//! Changing a dynamic as t.bilibili.com's editor does (dyn-home bundle
//! `index.*.js`: `_getEditData`, `getReq`, `feed/edit/dyn`). The
//! dynamic's detail gives its text nodes, pictures, title and topic,
//! `edit/infos` whether it may change now, and the whole dynamic goes
//! back through `feed/edit/dyn`, signed like `create/dyn`: what is not
//! asked is sent as it was, the pictures included.

use atp_core::cli::Change;
use atp_core::{Action, Edit, Error, ErrorCode, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::parse::https;
use crate::refs::DYNAMIC_URL;

const DETAIL: &str = "https://api.bilibili.com/x/polymer/web-dynamic/v1/detail";
const INFOS: &str = "https://api.bilibili.com/x/dynamic/feed/edit/infos";
const EDIT: &str = "https://api.bilibili.com/x/dynamic/feed/edit/dyn";
const FEATURES: &str = "itemOpusStyle,opusBigCover,onlyfansVote,endFooterHidden,decorationCard,onlyfansAssetsV2,ugcDelete";
/// `scene` of an edited dynamic with text or pictures.
const SCENE: u8 = 2;
/// `scene` of an edited repost.
const REPOST_SCENE: u8 = 4;
/// What the editor sends for a dynamic of pictures without text.
const PICTURES_ONLY: &str = "分享图片";

/// A dynamic as the editor loads it.
#[derive(Debug, Clone)]
struct Dynamic {
  id: String,
  /// The dynamic a repost reposts.
  repost_of: Option<String>,
  title: String,
  /// The text as `dyn_req.content.contents` takes it.
  contents: Vec<Value>,
  /// The text as shown.
  text: String,
  pictures: Vec<Value>,
  picture_mode: i64,
  topic: Option<Topic>,
  /// Declared AI-generated.
  aigc: bool,
  /// It carries a lottery or a vote, which a new text would drop.
  locked: bool,
}

#[derive(Debug, Clone)]
struct Topic {
  id: u64,
  name: Option<String>,
}

/// Change one of the account's dynamics.
pub async fn edit(api: &Api, id: &str, edit: &Edit) -> Result<Action> {
  if edit.cover.is_some() {
    return Err(Error::unsupported("edit --cover (dynamics)"));
  }
  if edit.visibility.is_some() {
    return Err(Error::unsupported("edit --visibility (dynamics)"));
  }
  api.require_login()?;
  let old = Dynamic::of(id, &detail(api, id).await?)?;
  api
    .get(INFOS)
    .arg("dyn_id", id)
    .arg("scene", 1)
    .send()
    .await?;
  let new = change(&old, edit)?;
  ask(&old, &new, edit.yes)?;
  let mut body = json!({ "dyn_id_str": id, "dyn_req": request(&new) });
  if let Some(orig) = &new.repost_of {
    body["web_repost_src"] = json!({ "dyn_id_str": orig });
  }
  api
    .post(EDIT)
    .query("platform", "web")
    .json(body)
    .dm()
    .wbi()
    .send()
    .await?;
  Ok(
    Action::done("edit", id)
      .with_id(id)
      .with_url(format!("{DYNAMIC_URL}{id}")),
  )
}

/// The dynamic as the web client's detail serves it (with `basic.editable`).
async fn detail(api: &Api, id: &str) -> Result<Value> {
  let data = api
    .get(DETAIL)
    .arg("id", id)
    .arg("timezone_offset", -480)
    .arg("platform", "web")
    .arg("gaia_source", "main_web")
    .arg("features", FEATURES)
    .arg("web_location", "333.1368")
    .dm()
    .wbi()
    .send()
    .await?;
  match data.at("item") {
    Value::Null => Err(Error::not_found(format!("dynamic {id} not found"))),
    item => Ok(item.clone()),
  }
}

impl Dynamic {
  fn of(id: &str, item: &Value) -> Result<Self> {
    let kind = item.str("type").unwrap_or_default();
    let body = item.at("modules.module_dynamic");
    match kind.as_str() {
      "DYNAMIC_TYPE_WORD" | "DYNAMIC_TYPE_DRAW" | "DYNAMIC_TYPE_FORWARD" => {}
      "DYNAMIC_TYPE_AV" => {
        let bvid = body.str("major.archive.bvid").unwrap_or_default();
        return Err(
          Error::input("this dynamic announces a video; edit the video instead")
            .with_hint(format!("atp bili edit {bvid}")),
        );
      }
      "DYNAMIC_TYPE_ARTICLE" => return Err(Error::unsupported("edit (articles)")),
      _ => {
        return Err(Error::unsupported(format!(
          "edit of a dynamic of type {kind}"
        )));
      }
    }
    if item.bool("basic.editable") != Some(true) {
      return Err(
        Error::new(
          ErrorCode::PermissionDenied,
          "Bilibili does not offer to edit this dynamic",
        )
        .with_hint("only your own dynamics change, and not while they are reviewed"),
      );
    }
    let opus = body.at("major.opus");
    let (title, nodes, pictures, picture_mode) = if opus.is_object() {
      (
        opus.str("title"),
        opus.list("summary.rich_text_nodes"),
        pictures(opus.list("pics"), "url"),
        opus.i64("style").unwrap_or(0),
      )
    } else {
      let draw = pictures(body.list("major.draw.items"), "src");
      (None, body.list("desc.rich_text_nodes"), draw, 0)
    };
    let contents = nodes.iter().map(content).collect::<Result<Vec<_>>>()?;
    Ok(Self {
      id: id.to_owned(),
      repost_of: (kind == "DYNAMIC_TYPE_FORWARD")
        .then(|| item.str("orig.id_str"))
        .flatten(),
      title: title.unwrap_or_default(),
      text: shown(&contents),
      locked: contents
        .iter()
        .any(|c| matches!(c.i64("type"), Some(3 | 4))),
      contents,
      pictures,
      picture_mode,
      topic: body.u64("topic.id").filter(|t| *t != 0).map(|id| Topic {
        id,
        name: body.str("topic.name"),
      }),
      aigc: item.bool("basic.aigc") == Some(true),
    })
  }
}

/// A rich text node of the detail as `dyn_req.content.contents` takes it
/// (the editor's `pZ`, then `u5`): mentions, lotteries, votes and emoji
/// keep their kind; links go back as their address.
fn content(node: &Value) -> Result<Value> {
  let text = node.str("text").unwrap_or_default();
  let rid = node.str("rid").unwrap_or_default();
  let (raw, kind, biz) = match node.str("type").unwrap_or_default().as_str() {
    "RICH_TEXT_NODE_TYPE_AT" => (text, 2, rid),
    "RICH_TEXT_NODE_TYPE_LOTTERY" => (text, 3, rid),
    "RICH_TEXT_NODE_TYPE_VOTE" => (text, 4, rid),
    "RICH_TEXT_NODE_TYPE_EMOJI" => (text, 9, String::new()),
    "RICH_TEXT_NODE_TYPE_GOODS" => {
      return Err(Error::unsupported("edit of dynamics with goods"));
    }
    "RICH_TEXT_NODE_TYPE_BV" => (
      https(node.str("jump_url").unwrap_or(text)),
      1,
      String::new(),
    ),
    "RICH_TEXT_NODE_TYPE_WEB" => (
      https(node.str("orig_text").unwrap_or(text)),
      1,
      String::new(),
    ),
    "RICH_TEXT_NODE_TYPE_VIEW_PICTURE" if !rid.is_empty() => {
      (format!("{DYNAMIC_URL}{rid}"), 1, String::new())
    }
    _ => (text, 1, String::new()),
  };
  Ok(json!({ "raw_text": raw, "type": kind, "biz_id": biz }))
}

/// Pictures as `dyn_req.pics` takes them; `src` names the address field.
fn pictures(list: &[Value], src: &str) -> Vec<Value> {
  list
    .iter()
    .filter_map(|p| {
      let mut picture = json!({
        "img_src": p.str(src)?,
        "img_width": p.at("width"),
        "img_height": p.at("height"),
        "img_size": p.at("size"),
      });
      if let Some(live) = p.str("live_url") {
        picture["live_video_src"] = live.into();
      }
      if !p.at("aigc").is_null() {
        picture["ai_gen_pic"] = p.at("aigc").clone();
      }
      Some(picture)
    })
    .collect()
}

fn shown(contents: &[Value]) -> String {
  contents.iter().filter_map(|c| c.str("raw_text")).collect()
}

/// The dynamic after the edit; what is not given stays as it was.
fn change(old: &Dynamic, edit: &Edit) -> Result<Dynamic> {
  let mut new = old.clone();
  if let Some(text) = &edit.text {
    if old.locked {
      return Err(Error::input(
        "this dynamic carries a lottery or a vote, which a new text would drop",
      ));
    }
    if text.trim().is_empty() && old.pictures.is_empty() {
      return Err(Error::input("a dynamic without pictures needs a text"));
    }
    new.contents = if text.is_empty() {
      Vec::new()
    } else {
      vec![json!({ "raw_text": text, "type": 1, "biz_id": "" })]
    };
    new.text = text.clone();
  }
  if let Some(title) = &edit.title {
    if old.repost_of.is_some() {
      return Err(Error::input("a repost has no title"));
    }
    new.title = title.trim().to_owned();
  }
  if let Some(topics) = &edit.topics {
    new.topic = match topics.as_slice() {
      [] => None,
      [one] => Some(topic(one, old.topic.as_ref())?),
      _ => return Err(Error::input("Bilibili dynamics take one topic")),
    };
  }
  Ok(new)
}

/// A topic given by its numeric id, with its name when it is the old one.
fn topic(given: &str, old: Option<&Topic>) -> Result<Topic> {
  let given = given.trim().trim_matches('#');
  let id = given.parse::<u64>().map_err(|_| {
    Error::input(format!(
      "Bilibili topics are numeric topic ids, got `{given}`"
    ))
  })?;
  let name = old.filter(|t| t.id == id).and_then(|t| t.name.clone());
  Ok(Topic { id, name })
}

/// Show the change and ask (or take `--yes`).
fn ask(old: &Dynamic, new: &Dynamic, yes: bool) -> Result<()> {
  let topic = |t: &Option<Topic>| match t {
    Some(Topic {
      name: Some(name), ..
    }) => format!("#{name}"),
    Some(Topic { id, .. }) => format!("#{id}"),
    None => String::new(),
  };
  let mut change = Change::new(format!("Bilibili dynamic {}:", old.id));
  if old.repost_of.is_none() {
    change.field("title", &old.title, &new.title);
  }
  change
    .field("text", &old.text, &new.text)
    .field("topic", &topic(&old.topic), &topic(&new.topic));
  if !old.pictures.is_empty() {
    change.row("pictures", &format!("{} (kept)", old.pictures.len()));
  }
  change.confirm(yes, "change this dynamic on Bilibili")
}

/// `dyn_req` as the editor builds it (`getReq`).
fn request(d: &Dynamic) -> Value {
  let mut contents = d.contents.clone();
  if contents.is_empty() && !d.pictures.is_empty() {
    contents.push(json!({ "raw_text": PICTURES_ONLY, "type": 1, "biz_id": "" }));
  }
  let scene = if d.repost_of.is_some() {
    REPOST_SCENE
  } else {
    SCENE
  };
  let mut req = json!({
    "content": { "contents": contents },
    "scene": scene,
    "meta": { "app_meta": { "from": "create.dynamic.web", "mobi_app": "web" } },
    "option": { "aigc": if d.aigc { 1 } else { 2 } },
  });
  if !d.title.is_empty() {
    req["content"]["title"] = d.title.clone().into();
  }
  if !d.pictures.is_empty() {
    req["pics"] = json!(d.pictures);
    req["option"]["pic_mode"] = d.picture_mode.into();
  }
  if let Some(topic) = &d.topic {
    req["topic"] = match &topic.name {
      Some(name) => json!({ "id": topic.id, "name": name, "from_topic_id": 0 }),
      None => json!({ "id": topic.id }),
    };
  }
  req
}

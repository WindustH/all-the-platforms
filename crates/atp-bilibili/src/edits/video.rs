//! Changing a published video (稿件) as the creator center's edit page
//! does (member.bilibili.com/york/videoup, bundle `index.*.js`:
//! `getArchiveInfo`, `getBasicParams`, `getSubmitParams`, `editArchive`).
//! The page loads the whole archive (`archive/view`) and sends the whole
//! record back through `vu/web/edit`, not a difference: every field below
//! is copied from what was loaded, and only the asked ones change. A new
//! cover goes up through `cover/up` first. Bilibili reviews the video
//! again after a change.

use std::path::Path;

use atp_core::cli::Change;
use atp_core::file::Image;
use atp_core::http::Part;
use atp_core::output::note;
use atp_core::{Action, Edit, Error, ErrorCode, Result, Value, ValueExt, json};
use base64::Engine as _;

use crate::api::Api;
use crate::refs::Video;

const VIEW: &str = "https://member.bilibili.com/x/vupre/web/archive/view";
const EDIT: &str = "https://member.bilibili.com/x/vu/web/edit";
const COVER: &str = "https://member.bilibili.com/x/vu/web/cover/up";
const MEMBER: &str = "https://member.bilibili.com";
const PAGE: &str = "https://member.bilibili.com/platform/upload/video/frame";
const TITLE_MAX: usize = 80;
const TEXT_MAX: usize = 2000;
const TAGS_MAX: usize = 10;
/// `web_os` of the browser ATP's User-Agent names (macOS): 1 Windows,
/// 2 macOS, 3 others.
const WEB_OS: u8 = 2;
/// `copyright` of a reposted (转载) video.
const REPRINT: i64 = 2;

/// What an edit may change on a video.
#[derive(Debug, Clone)]
struct Fields {
  title: String,
  /// The description (简介).
  text: String,
  tags: Vec<String>,
  /// Only the uploader sees it (`is_only_self`).
  private: bool,
}

/// Change one of the account's videos.
pub async fn edit(api: &Api, v: &Video, edit: &Edit) -> Result<Action> {
  api.require_login()?;
  let referer = format!("{PAGE}?type=edit&bvid={}", v.bvid);
  let view = api
    .get(VIEW)
    .arg("bvid", &v.bvid)
    .arg("topic_grey", 1)
    .header("referer", referer.clone())
    .send()
    .await?;
  refuse(&view, edit)?;
  let old = fields(view.at("archive"));
  let new = change(view.at("archive"), &old, edit)?;
  ask(v, &old, &new, edit)?;
  let cover = match &edit.cover {
    Some(path) => Some(cover(api, path, &referer).await?),
    None => None,
  };
  let body = body(&view, &new, cover, &api.csrf()?);
  api
    .post(EDIT)
    .json(body)
    .header("referer", referer)
    .header("origin", MEMBER)
    .send()
    .await?;
  Ok(
    Action::done("edit", &v.bvid)
      .with_id(&v.bvid)
      .with_url(v.url()),
  )
}

/// Videos whose record holds what ATP does not send back (staff,
/// charging, commercial orders ...), and edits Bilibili does not take now.
fn refuse(view: &Value, edit: &Edit) -> Result<()> {
  let a = view.at("archive");
  if a.u64("aid").is_none() || view.list("videos").is_empty() {
    return Err(Error::not_found(
      "no such video among this account's uploads",
    ));
  }
  let ordered =
    truthy(a.at("adorder_id")) || truthy(a.at("porder.id")) || truthy(a.at("porder.flow_id"));
  let kind = if !view.list("staffs").is_empty() {
    Some("joint submissions (联合投稿)")
  } else if truthy(a.at("charging_pay")) {
    Some("videos for chargers only (充电专属)")
  } else if a.i64("interactive") == Some(1) {
    Some("interactive videos (互动视频)")
  } else if ordered {
    Some("videos with a commercial order (商单)")
  } else {
    None
  };
  if let Some(kind) = kind {
    return Err(Error::unsupported(format!("edit of {kind}")));
  }
  let reason = if truthy(a.at("political_media")) && !truthy(a.at("political_editable")) {
    Some("the video cannot change before it opens (开放浏览前您的稿件暂无法编辑)")
  } else if truthy(a.at("attrs.is_premiere")) && !truthy(a.at("attrs.is_played")) {
    Some("a premiere cannot change before it has played (首映结束之后才可以编辑)")
  } else if edit.cover.is_some() && !a.list("ab_cover_info.ab_covers").is_empty() {
    Some("a cover test is running (封面测试中，暂不支持编辑封面)")
  } else {
    None
  };
  match reason {
    Some(reason) => Err(Error::new(
      ErrorCode::PermissionDenied,
      format!("Bilibili: {reason}"),
    )),
    None => Ok(()),
  }
}

fn fields(a: &Value) -> Fields {
  let own: Vec<String> = a
    .str("tag")
    .unwrap_or_default()
    .split(',')
    .map(str::to_owned)
    .collect();
  Fields {
    title: a.str("title").unwrap_or_default(),
    text: a.str("desc").unwrap_or_default(),
    tags: tags(a, &own),
    private: a.i64("is_only_self") == Some(1),
  }
}

/// The tags as the edit page lists them: the joined topic's (参与话题)
/// first, then `own`, without repeats.
fn tags(a: &Value, own: &[String]) -> Vec<String> {
  let mut tags: Vec<String> = Vec::new();
  let topic = a.str("topic_name");
  for tag in topic.iter().chain(own) {
    let tag = tag.trim().trim_start_matches('#').trim();
    if !tag.is_empty() && !tags.iter().any(|t| t == tag) {
      tags.push(tag.to_owned());
    }
  }
  tags
}

/// The fields after the edit; what is not given stays as it was.
fn change(a: &Value, old: &Fields, edit: &Edit) -> Result<Fields> {
  let title = edit
    .title
    .as_deref()
    .map_or_else(|| old.title.clone(), |t| t.trim().to_owned());
  let tags = match edit.topics.as_deref() {
    None => old.tags.clone(),
    Some([]) => return Err(Error::input("a Bilibili video keeps at least one tag")),
    Some(given) => tags(a, given),
  };
  let private = match edit.visibility.as_deref() {
    None => old.private,
    Some("private") => true,
    Some("public") => false,
    Some(other) => {
      return Err(Error::unsupported(format!(
        "edit --visibility {other} (Bilibili videos are public or private)"
      )));
    }
  };
  let new = Fields {
    title,
    text: edit.text.clone().unwrap_or_else(|| old.text.clone()),
    tags,
    private,
  };
  if new.title.is_empty() {
    return Err(Error::input("a Bilibili video needs a title"));
  }
  if new.title.chars().count() > TITLE_MAX {
    return Err(Error::input(format!(
      "a Bilibili video's title takes at most {TITLE_MAX} characters"
    )));
  }
  if new.text.chars().count() > TEXT_MAX {
    return Err(Error::input(format!(
      "a Bilibili video's description takes at most {TEXT_MAX} characters"
    )));
  }
  if new.tags.len() > TAGS_MAX {
    return Err(Error::input(format!(
      "a Bilibili video takes at most {TAGS_MAX} tags (its topic's included)"
    )));
  }
  Ok(new)
}

/// Show the change and ask (or take `--yes`).
fn ask(v: &Video, old: &Fields, new: &Fields, edit: &Edit) -> Result<()> {
  let visibility = |private: bool| if private { "private" } else { "public" };
  let mut change = Change::new(format!("Bilibili video {}:", v.bvid));
  change
    .field("title", &old.title, &new.title)
    .field("text", &old.text, &new.text)
    .field("tags", &old.tags.join(", "), &new.tags.join(", "))
    .field(
      "visibility",
      visibility(old.private),
      visibility(new.private),
    );
  if let Some(path) = &edit.cover {
    change.row("cover", &path.display().to_string());
  }
  change.row("review", "Bilibili reviews the video again");
  change.confirm(edit.yes, "change this video on Bilibili")
}

/// Upload a new cover; its address.
async fn cover(api: &Api, path: &Path, referer: &str) -> Result<String> {
  let image = Image::read(path).await?;
  if !matches!(image.mime, "image/jpeg" | "image/png") {
    return Err(Error::input("a Bilibili cover is a JPEG or PNG image"));
  }
  let data = base64::engine::general_purpose::STANDARD.encode(&image.data);
  let cover = format!("data:{};base64,{data}", image.mime);
  note("uploading the cover");
  let v = api
    .post(COVER)
    .multipart(vec![Part::text("cover", cover)])
    .header("referer", referer.to_owned())
    .header("origin", MEMBER)
    .send()
    .await?;
  v.str("url")
    .ok_or_else(|| Error::upstream("the cover upload returned no address"))
}

/// The record `vu/web/edit` takes: the archive as loaded, with the new
/// fields.
fn body(view: &Value, new: &Fields, cover: Option<String>, csrf: &str) -> Value {
  let a = view.at("archive");
  let watermark = if a.i64("copyright") == Some(REPRINT) {
    0
  } else {
    view.i64("watermark.state").unwrap_or(0)
  };
  // 1 hides the video from the space, 2 (the default) shows it.
  let space_hidden = view
    .i64("space_hidden")
    .or_else(|| a.i64("space_hidden"))
    .filter(|s| *s != 0)
    .unwrap_or(2);
  let mut body = json!({
    "aid": a.at("aid"),
    "title": new.title,
    "copyright": a.at("copyright"),
    "tid": a.at("tid"),
    "tag": new.tags.join(","),
    "mission_id": a.i64("mission_id").unwrap_or(0),
    "desc_format_id": a.i64("desc_format_id").unwrap_or(0),
    "desc": new.text,
    "dynamic": a.str("dynamic").unwrap_or_default(),
    "recreate": recreate(a),
    "interactive": 0,
    "videos": videos(view),
    "new_web_edit": 1,
    "handle_staff": false,
    "topic_grey": 1,
    "is_only_self": i32::from(new.private),
    "space_hidden": space_hidden,
    "watermark": { "state": watermark },
    "no_reprint": a.i64("no_reprint").unwrap_or(0),
    "is_360": i32::from(a.i64("is_360") == Some(1)),
    "web_os": WEB_OS,
    "csrf": csrf,
  });
  covers(a, cover, &mut body);
  optional(a, new, &mut body);
  body
}

/// The covers: a new one replaces the 16:9 cover and clears the 4:3 crop
/// (the edit page sends none where it has no 4:3 editor either).
fn covers(a: &Value, cover: Option<String>, body: &mut Value) {
  let tested = !a.list("ab_cover_info.ab_covers").is_empty();
  body["is_ab_cover"] = tested.into();
  body["ab_cover_info"] = a.at("ab_cover_info").clone();
  match cover {
    Some(url) => {
      body["cover"] = url.into();
      body["cover43"] = "".into();
      body["ai_cover"] = false.into();
    }
    None => {
      body["cover"] = a.str("cover").unwrap_or_default().into();
      body["cover43"] = a.str("cover43").unwrap_or_default().into();
      body["ai_cover"] = truthy(a.at("ai_cover")).into();
    }
  }
}

/// Fields the edit page sends only when the archive has them.
fn optional(a: &Value, new: &Fields, body: &mut Value) {
  if a.i64("copyright") == Some(REPRINT) {
    body["source"] = a.str("source").unwrap_or_default().into();
  }
  if let Some(id) = a.i64("creation_statement.id") {
    body["creation_statement"] = json!({ "id": id });
  }
  if !a.at("human_type2").is_null() {
    body["human_type2"] = a.at("human_type2").clone();
  }
  if let Some(topic) = a.u64("topic_id").filter(|t| *t != 0) {
    let mut detail = json!({ "from_topic_id": topic });
    if let Some(from) = a.str("topic_from") {
      detail["from_source"] = format!("arc.web.{from}").into();
    }
    body["topic_id"] = topic.into();
    body["topic_detail"] = detail;
  }
  if let Some(dtime) = a.i64("dtime").filter(|d| *d > 0) {
    body["dtime"] = dtime.into();
  }
  // With @-mentions the page sends the parts too; a new text has none.
  if a.str("desc").unwrap_or_default() == new.text && mentions(a.at("desc_v2")) {
    body["desc_v2"] = a.at("desc_v2").clone();
  }
  if mentions(a.at("dynamic_v2")) {
    body["dynamic_v2"] = a.at("dynamic_v2").clone();
  }
  if a.i64("is_dolby") == Some(1) {
    body["dolby"] = 1.into();
  }
  if a.i64("lossless_music") == Some(1) {
    body["lossless_music"] = 1.into();
  }
}

/// The parts (分P) as loaded, in their order.
fn videos(view: &Value) -> Vec<Value> {
  view
    .list("videos")
    .iter()
    .map(|part| {
      json!({
        "filename": part.str("filename").unwrap_or_default(),
        "title": part.str("title").unwrap_or_default(),
        "desc": "",
        "cid": part.u64("cid").unwrap_or(0),
      })
    })
    .collect()
}

/// `recreate.switch` (or a plain `recreate`) 1 stays 1; anything else is -1.
fn recreate(a: &Value) -> i64 {
  match a.i64("recreate.switch").or_else(|| a.i64("recreate")) {
    Some(1) => 1,
    _ => -1,
  }
}

/// Text parts (`desc_v2`, `dynamic_v2`) holding an @-mention (type 2).
fn mentions(parts: &Value) -> bool {
  parts
    .as_array()
    .is_some_and(|parts| parts.iter().any(|p| p.i64("type") == Some(2)))
}

/// A flag or id that is set: true, a number other than 0, a string other
/// than "" and "0", a non-empty list or object.
fn truthy(v: &Value) -> bool {
  match v {
    Value::Bool(b) => *b,
    Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.0),
    Value::String(s) => !s.is_empty() && s != "0",
    Value::Array(a) => !a.is_empty(),
    Value::Object(o) => !o.is_empty(),
    Value::Null => false,
  }
}

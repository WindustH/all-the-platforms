//! Posts: one post and what hangs off it (the post with its long text,
//! comments and their reply threads, who liked it, its reposts), and
//! publishing, reposting and deleting as weibo.com's composer does, with
//! pictures uploaded to picupload first.

use std::path::Path;

use atp_core::file::Image;
use atp_core::{
  Action, Comment, Draft, Error, Page, PageReq, Post, Result, User, Value, ValueExt, json,
};

use crate::api::Api;
use crate::parse;
use crate::refs::{self, mid_to_bid, post_url};

/// The raw post, its long text filled in.
pub async fn show(api: &Api, arg: &str) -> Result<Value> {
  let mid = refs::mid(arg)?;
  let bid = mid_to_bid(&mid).unwrap_or_else(|| mid.clone());
  let mut v = api
    .get(
      "/ajax/statuses/show",
      &[("id", &bid), ("locale", "zh-CN"), ("isGetLongText", "true")],
    )
    .await?;
  if v.str("idstr").is_none() && v.str("mid").is_none() {
    return Err(Error::not_found(format!("no Weibo post {arg}")));
  }
  if v.bool("isLongText") == Some(true) && v.str("longText.longTextContent").is_none() {
    let long = api
      .get("/ajax/statuses/longtext", &[("id", &bid)])
      .await
      .ok()
      .and_then(|l| l.str("data.longTextContent"));
    if let (Some(text), Some(obj)) = (long, v.as_object_mut()) {
      obj.insert(
        "longText".into(),
        serde_json::json!({ "longTextContent": text }),
      );
    }
  }
  Ok(v)
}

pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let v = show(api, arg).await?;
  parse::post(&v).ok_or_else(|| Error::upstream("Weibo returned a post without an id"))
}

/// Comments, by heat (`hot`, the default) or by time (`time`).
pub async fn comments(
  api: &Api,
  arg: &str,
  sort: Option<&str>,
  page: &PageReq,
) -> Result<Page<Comment>> {
  let mid = refs::mid(arg)?;
  let flow = if sort == Some("time") { "1" } else { "0" };
  let max_id = page.cursor.clone().unwrap_or_default();
  let count = page.size_within(20).to_string();
  let mut query = vec![
    ("is_reload", "1"),
    ("id", mid.as_str()),
    ("is_show_bulletin", "2"),
    ("is_mix", "0"),
    ("count", count.as_str()),
    ("fetch_level", "0"),
    ("flow", flow),
    ("locale", "zh-CN"),
  ];
  if !max_id.is_empty() {
    query.push(("max_id", max_id.as_str()));
  }
  let v = api.get("/ajax/statuses/buildComments", &query).await?;
  Ok(comment_page(&v))
}

/// Replies under one comment.
pub async fn replies(api: &Api, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
  let cid = comment.trim();
  if cid.is_empty() || !cid.bytes().all(|b| b.is_ascii_digit()) {
    return Err(Error::input(format!("not a Weibo comment id: {comment}")));
  }
  let max_id = page.cursor.clone().unwrap_or_else(|| "0".into());
  let count = page.size_within(20).to_string();
  let v = api
    .get(
      "/ajax/statuses/buildComments",
      &[
        ("flow", "0"),
        ("is_reload", "1"),
        ("id", cid),
        ("is_show_bulletin", "2"),
        ("is_mix", "1"),
        ("fetch_level", "1"),
        ("max_id", &max_id),
        ("count", &count),
        ("locale", "zh-CN"),
      ],
    )
    .await?;
  Ok(comment_page(&v))
}

fn comment_page(v: &Value) -> Page<Comment> {
  let items: Vec<Comment> = v.list("data").iter().filter_map(parse::comment).collect();
  let next = v.str("max_id").filter(|m| m != "0" && !items.is_empty());
  Page::new(items, next)
}

/// Accounts that liked a post (the newest first).
pub async fn likers(api: &Api, arg: &str, page: &PageReq) -> Result<Page<User>> {
  let mid = refs::mid(arg)?;
  let no = page.number_or(1);
  let count = page.size_within(20);
  let v = api
    .get(
      "/ajax/statuses/likeShow",
      &[
        ("id", &mid),
        ("attitude_type", "0"),
        ("attitude_enable", "1"),
        ("page", &no.to_string()),
        ("count", &count.to_string()),
      ],
    )
    .await?;
  let mut items = Vec::new();
  for entry in v.list("data") {
    if let Some(mut u) = parse::user(entry.at("user")) {
      if let Some(kind) = entry.str("attitude") {
        u.extra.insert("reaction".into(), kind.into());
      }
      items.push(u);
    }
  }
  let total = v.u64("total_number").unwrap_or(0);
  let more = !items.is_empty() && no * (count as u64) < total;
  Ok(Page::new(items, more.then(|| (no + 1).to_string())))
}

/// Reposts of a post.
pub async fn reposts(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let mid = refs::mid(arg)?;
  let no = page.number_or(1);
  let v = api
    .get(
      "/ajax/statuses/repostTimeline",
      &[
        ("id", &mid),
        ("page", &no.to_string()),
        ("moduleID", "feed"),
        ("count", &page.size_within(20).to_string()),
      ],
    )
    .await?;
  let items: Vec<Post> = v.list("data").iter().filter_map(parse::post).collect();
  let max_page = v.u64("max_page").unwrap_or(0);
  let more = !items.is_empty() && no < max_page;
  Ok(Page::new(items, more.then(|| (no + 1).to_string())))
}

// ── writing ─────────────────────────────────────────────────────────────

const UPLOAD: &str = "https://picupload.weibo.com/interface/pic_upload.php";

/// Pictures one post can carry.
const MAX_PICS: usize = 18;

/// Upload one picture; its `pid`.
async fn upload(api: &Api, path: &Path) -> Result<String> {
  let image = Image::read(path).await?;
  let resp = api
    .ctx
    .http
    .post(UPLOAD)
    .query("app", "miniblog")
    .query("s", "json")
    .query("p", 1)
    .query("data", 1)
    .header("referer", crate::api::HOME)
    .header("origin", "https://weibo.com")
    .bytes(image.data, "application/octet-stream")
    .send()
    .await?;
  let text = resp.text();
  let body = text.find('{').map_or(text.as_str(), |i| &text[i..]);
  let v: Value = serde_json::from_str(body).unwrap_or_default();
  if v.str("code").as_deref() != Some("A00006") {
    return Err(Error::upstream(format!(
      "picture upload failed ({}): {}",
      v.str("code").unwrap_or_else(|| resp.status.to_string()),
      path.display()
    )));
  }
  v.str("data.pics.pic_1.pid")
    .ok_or_else(|| Error::upstream("picture upload returned no pid"))
}

/// `#topic#` for every `--topic` the text does not already carry.
fn with_topics(text: &str, topics: &[String]) -> String {
  let mut out = text.to_owned();
  for t in topics {
    let tag = format!("#{}#", t.trim().trim_matches('#'));
    if !out.contains(&tag) {
      if !out.is_empty() {
        out.push(' ');
      }
      out.push_str(&tag);
    }
  }
  out
}

pub async fn publish(api: &Api, draft: &Draft) -> Result<Action> {
  api.require_login()?;
  if draft.reply_to.is_some() {
    return Err(Error::unsupported("post --reply-to").with_hint("use `comment` to reply"));
  }
  if let Some(quoted) = &draft.quote {
    if !draft.images.is_empty() {
      return Err(Error::unsupported("post --quote with --image"));
    }
    let text = with_topics(&draft.text, &draft.topics);
    return repost(api, quoted, &text, false).await;
  }
  if draft.images.len() > MAX_PICS {
    return Err(Error::input(format!(
      "Weibo takes at most {MAX_PICS} pictures"
    )));
  }
  let mut pics = Vec::new();
  for path in &draft.images {
    pics.push(json!({"type": "pic", "pid": upload(api, path).await?}));
  }
  let text = with_topics(&draft.text, &draft.topics);
  let content = match &draft.title {
    Some(title) if !text.is_empty() => format!("{title}\n{text}"),
    Some(title) => title.clone(),
    None => text,
  };
  if content.trim().is_empty() && pics.is_empty() {
    return Err(Error::input("nothing to post"));
  }
  let pic_id = if pics.is_empty() {
    String::new()
  } else {
    Value::from(pics).to_string()
  };
  api.write_pause().await;
  let v = api
    .post_form(
      "/ajax/statuses/update",
      &[
        ("content", &content),
        ("pic_id", &pic_id),
        ("visible", "0"),
        ("share_id", ""),
        ("media", ""),
        ("vote", ""),
      ],
    )
    .await?;
  created(&v, "publish", None)
}

/// The created post of a write response (`data` or the top level).
fn created(v: &Value, name: &str, target: Option<&str>) -> Result<Action> {
  if let Some(msg) = v.str("error").or_else(|| v.str("data.error")) {
    return Err(Error::upstream(msg));
  }
  let data = if v.at("data").is_object() {
    v.at("data")
  } else {
    v
  };
  let id = data.first_str(&["idstr", "mid", "id"]);
  let mut action = Action::done(
    name,
    target.map(str::to_owned).or(id.clone()).unwrap_or_default(),
  );
  if let Some(id) = id {
    let uid = data.str("user.idstr");
    let bid = data.str("mblogid").or_else(|| mid_to_bid(&id));
    action = action
      .with_url(post_url(uid.as_deref(), &id, bid.as_deref()))
      .with_id(id);
  }
  if let Some(msg) = v.str("msg").filter(|m| !m.is_empty()) {
    action = action.with_message(msg);
  }
  Ok(action)
}

/// Repost (转发) with an optional text; `also_comment` posts it as a comment too.
pub async fn repost(api: &Api, arg: &str, text: &str, also_comment: bool) -> Result<Action> {
  api.require_login()?;
  let mid = refs::mid(arg)?;
  api.write_pause().await;
  let v = api
    .post_form(
      "/ajax/statuses/normal_repost",
      &[
        ("id", &mid),
        ("comment", text),
        ("pic_id", ""),
        ("is_repost", "0"),
        ("comment_ori", "0"),
        ("is_comment", if also_comment { "1" } else { "0" }),
      ],
    )
    .await?;
  created(&v, "repost", Some(&mid))
}

pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  api.require_login()?;
  let mid = refs::mid(arg)?;
  api.write_pause().await;
  api
    .post_json("/ajax/statuses/destroy", &json!({"id": mid}))
    .await?;
  Ok(Action::done("delete", &mid))
}

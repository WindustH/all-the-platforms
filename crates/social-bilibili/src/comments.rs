//! Comment sections: listing (main replies / one thread) and writing.
//!
//! Every commentable resource has an `(oid, type)` pair: a video is
//! `(aid, 1)`, a dynamic tells its own pair in `basic.comment_id_str/_type`.

use social_core::{Action, Comment, Error, Page, PageReq, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::paging::Pn;
use crate::refs::{self, PostRef};
use crate::{articles, dynamics, parse};

const LIST: &str = "https://api.bilibili.com/x/v2/reply";
const MAIN: &str = "https://api.bilibili.com/x/v2/reply/wbi/main";
const THREAD: &str = "https://api.bilibili.com/x/v2/reply/reply";
const ADD: &str = "https://api.bilibili.com/x/v2/reply/add";
const DEL: &str = "https://api.bilibili.com/x/v2/reply/del";

/// The comment section of a post.
pub struct Section {
  pub oid: String,
  pub kind: u64,
  /// The post it belongs to (BV id or dynamic id), for action results.
  pub post: String,
}

pub async fn section(api: &Api, post: &PostRef) -> Result<Section> {
  match post {
    PostRef::Video(v) => Ok(Section {
      oid: v.aid.to_string(),
      kind: 1,
      post: v.bvid.clone(),
    }),
    PostRef::Article(id) => Ok(Section {
      oid: id.to_string(),
      kind: articles::COMMENT_TYPE,
      post: format!("cv{id}"),
    }),
    PostRef::Dynamic(id) => {
      let item = dynamics::detail(api, id).await?;
      match (
        item.str("basic.comment_id_str"),
        item.u64("basic.comment_type"),
      ) {
        (Some(oid), Some(kind)) => Ok(Section {
          oid,
          kind,
          post: id.clone(),
        }),
        _ => Err(Error::not_found(format!(
          "dynamic {id} has no comment section"
        ))),
      }
    }
  }
}

/// Main replies: `hot` (default, by likes, paged by number) or `time` (newest
/// first, offset cursor; anonymous visitors only get the first few).
pub async fn list(
  api: &Api,
  s: &Section,
  sort: Option<&str>,
  page: &PageReq,
) -> Result<Page<Comment>> {
  if sort == Some("time") {
    return latest(api, s, page).await;
  }
  let pn = Pn::of(page, 20);
  let data = api
    .get(LIST)
    .arg("oid", &s.oid)
    .arg("type", s.kind)
    .arg("pn", pn.pn)
    .arg("ps", pn.ps)
    .arg("sort", 1)
    .send()
    .await?;
  let pinned = (pn.pn == 1).then(|| data.at("upper.top"));
  let comments = with_pinned(pinned, data.list("replies"));
  Ok(pn.page(comments, pn.before(data.u64("page.count"))))
}

async fn latest(api: &Api, section: &Section, page: &PageReq) -> Result<Page<Comment>> {
  let offset = page.cursor.as_deref().unwrap_or_default();
  let data = api
    .get(MAIN)
    .arg("oid", &section.oid)
    .arg("type", section.kind)
    .arg("mode", 2)
    .arg("pagination_str", json!({ "offset": offset }).to_string())
    .arg("plat", 1)
    .arg("web_location", 1315875)
    .wbi()
    .send()
    .await?;
  let pinned = offset.is_empty().then(|| data.at("top_replies.0"));
  let comments = with_pinned(pinned, data.list("replies"));
  let next = data
    .str("cursor.pagination_reply.next_offset")
    .filter(|_| data.bool("cursor.is_end") == Some(false));
  Ok(Page::new(comments, next))
}

/// Replies, preceded by the pinned one (marked `extra.pinned`) when there is one.
fn with_pinned(pinned: Option<&Value>, replies: &[Value]) -> Vec<Comment> {
  let mut out: Vec<Comment> = Vec::new();
  if let Some(top) = pinned.filter(|t| t.is_object()) {
    let mut c = parse::comment(top);
    c.extra.insert("pinned".into(), json!(true));
    out.push(c);
  }
  for c in replies.iter().map(parse::comment) {
    if !out.iter().any(|p| p.id == c.id) {
      out.push(c);
    }
  }
  out
}

/// Replies inside the thread of root comment `root`.
pub async fn thread(
  api: &Api,
  section: &Section,
  root: &str,
  page: &PageReq,
) -> Result<Page<Comment>> {
  let pn = Pn::of(page, 20);
  let data = api
    .get(THREAD)
    .arg("oid", &section.oid)
    .arg("type", section.kind)
    .arg("root", root)
    .arg("pn", pn.pn)
    .arg("ps", pn.ps)
    .send()
    .await?;
  let replies = data.list("replies").iter().map(parse::comment).collect();
  Ok(pn.page(replies, pn.before(data.u64("page.count"))))
}

/// Comment on the post, or reply to `ROOT` / `ROOT:PARENT`.
pub async fn add(
  api: &Api,
  section: &Section,
  text: &str,
  reply_to: Option<&str>,
) -> Result<Action> {
  let mut call = api
    .post(ADD)
    .arg("oid", &section.oid)
    .arg("type", section.kind)
    .arg("message", text)
    .arg("plat", 1)
    .arg("statistics", r#"{"appId":100,"platform":5}"#)
    .arg("gaia_source", "main_web");
  if let Some(target) = reply_to {
    let (root, parent) = refs::reply_target(target)?;
    call = call.arg("root", root).arg("parent", parent);
  }
  let data = call.dm().wbi().send().await?;
  let verb = if reply_to.is_some() {
    "reply"
  } else {
    "comment"
  };
  let action = Action::done(verb, &section.post);
  Ok(match data.first_str(&["rpid_str", "rpid"]) {
    Some(id) => action.with_id(id),
    None => action,
  })
}

pub async fn delete(api: &Api, section: &Section, rpid: &str) -> Result<Action> {
  api
    .post(DEL)
    .arg("oid", &section.oid)
    .arg("type", section.kind)
    .arg("rpid", rpid)
    .send()
    .await?;
  Ok(Action::done("delete-comment", &section.post).with_id(rpid))
}

/// Like a comment, or with `dislike` mark it down (踩); `undo` removes either.
pub async fn react(
  api: &Api,
  s: &Section,
  rpid: &str,
  dislike: bool,
  undo: bool,
) -> Result<Action> {
  let url = if dislike {
    "https://api.bilibili.com/x/v2/reply/hate"
  } else {
    "https://api.bilibili.com/x/v2/reply/action"
  };
  api
    .post(url)
    .arg("oid", &s.oid)
    .arg("type", s.kind)
    .arg("rpid", rpid)
    .arg("action", u8::from(!undo))
    .send()
    .await?;
  let verb = match (dislike, undo) {
    (false, false) => "comment-like",
    (false, true) => "comment-unlike",
    (true, false) => "comment-dislike",
    (true, true) => "comment-undislike",
  };
  Ok(Action::done(verb, &s.post).with_id(rpid))
}

/// Pin (or with `undo` unpin) a comment under your own post.
pub async fn pin(api: &Api, section: &Section, rpid: &str, undo: bool) -> Result<Action> {
  api
    .post("https://api.bilibili.com/x/v2/reply/top")
    .arg("oid", &section.oid)
    .arg("type", section.kind)
    .arg("rpid", rpid)
    .arg("action", u8::from(!undo))
    .send()
    .await?;
  let verb = if undo { "comment-unpin" } else { "comment-pin" };
  Ok(Action::done(verb, &section.post).with_id(rpid))
}

//! A thread: the first floor is the post, the others are its comments, and
//! the replies inside a floor (楼中楼) are the replies of a comment.

use social_core::{Comment, Error, Media, Page, PageReq, Post, Result, Value, ValueExt};

use crate::api::{Client, Params};
use crate::{parse, refs};

/// `asc`: oldest floor first; `desc`: newest first; `author`: only the
/// original poster's floors (只看楼主).
pub const COMMENT_SORTS: &[&str] = &["asc", "desc", "author"];

/// One page of a thread (`pb`): thread, forum, floors and their authors.
async fn pb(c: &Client, tid: &str, pn: u64, rn: usize, sort: &str) -> Result<Value> {
  let desc = if sort == "desc" { "1" } else { "0" };
  let params: Params = vec![
    ("kz", tid.to_owned()),
    ("pn", pn.to_string()),
    ("rn", rn.to_string()),
    ("with_floor", "1".into()),
    ("floor_rn", "3".into()),
    ("lz", if sort == "author" { "1" } else { "0" }.into()),
    ("r", desc.into()),
    ("last", desc.into()),
    ("back", "0".into()),
    ("mark", "0".into()),
    ("st_type", "tb_frslist".into()),
    ("scr_dip", "3.0".into()),
    ("scr_h", "2400".into()),
    ("scr_w", "1080".into()),
  ];
  let v = c.call("/c/f/pb/page", params).await?;
  if v.str("thread.id").is_none() {
    return Err(Error::not_found(format!("no Tieba thread {tid}")));
  }
  Ok(v)
}

fn is_first(p: &Value) -> bool {
  p.u64("floor") == Some(1)
}

/// The thread with the text and media of its first floor.
pub async fn read(c: &Client, arg: &str) -> Result<Post> {
  let r = refs::post(arg)?;
  let v = pb(c, &r.tid, 1, 2, "asc").await?;
  let users = parse::users(v.list("user_list"));
  let mut post = parse::thread(v.at("thread"), &users);
  if let Some(first) = v.list("post_list").iter().find(|p| is_first(p)) {
    let body = parse::body(first.list("content"));
    post.text = Some(body.text).filter(|t| !t.is_empty());
    let mut media = body.media;
    let rest: Vec<Media> = post
      .media
      .drain(..)
      .filter(|m| !media_has(&media, m))
      .collect();
    media.extend(rest);
    post.media = media;
    if post.author.is_none() {
      post.author = parse::floor(first, &users, &r.tid).author;
    }
  }
  if let Some(name) = v.str("forum.name") {
    post.extra.insert("forum".into(), name.into());
  }
  if let Some(fid) = v.str("forum.id") {
    post.extra.insert("forum_id".into(), fid.into());
  }
  if let Some(n) = v.u64("thread.collect_num") {
    post.metrics.favorites = Some(n);
  }
  Ok(post)
}

fn media_has(list: &[Media], m: &Media) -> bool {
  list.iter().any(|x| x.url == m.url)
}

/// One page of the floors after the first.
pub async fn comments(
  c: &Client,
  arg: &str,
  sort: Option<&str>,
  req: &PageReq,
) -> Result<Page<Comment>> {
  let r = refs::post(arg)?;
  let pn = req.number_or(1);
  let v = pb(c, &r.tid, pn, 30, sort.unwrap_or("asc")).await?;
  let users = parse::users(v.list("user_list"));
  let items: Vec<Comment> = v
    .list("post_list")
    .iter()
    .filter(|p| !is_first(p))
    .map(|p| parse::floor(p, &users, &r.tid))
    .collect();
  let page = v.at("page");
  let more = page.str("has_more").as_deref() == Some("1")
    || page.u64("current_page") < page.u64("total_page");
  Ok(Page::new(items, more.then(|| (pn + 1).to_string())))
}

/// One page of the replies inside a floor.
pub async fn replies(c: &Client, arg: &str, floor: &str, req: &PageReq) -> Result<Page<Comment>> {
  let r = refs::post(arg)?;
  let pid = refs::post(floor)
    .ok()
    .and_then(|f| f.pid)
    .unwrap_or_else(|| floor.trim().to_owned());
  let pn = req.number_or(1);
  let params: Params = vec![
    ("kz", r.tid.clone()),
    ("pid", pid),
    ("pn", pn.to_string()),
    ("rn", req.size_within(30).to_string()),
  ];
  let v = c.call("/c/f/pb/floor", params).await?;
  let none = parse::Users::new();
  let items: Vec<Comment> = v
    .list("subpost_list")
    .iter()
    .map(|s| parse::floor(s, &none, &r.tid))
    .collect();
  let page = v.at("page");
  let more = page.u64("current_page") < page.u64("total_page") && !items.is_empty();
  Ok(Page::new(items, more.then(|| (pn + 1).to_string())))
}

/// Forum id and name, title and first floor id of a thread, for writes.
pub struct Context {
  pub fid: String,
  pub forum: String,
  pub first_pid: String,
}

pub async fn context(c: &Client, tid: &str) -> Result<Context> {
  let v = pb(c, tid, 1, 2, "asc").await?;
  let first_pid = v
    .str("thread.post_id")
    .or_else(|| {
      v.list("post_list")
        .iter()
        .find(|p| is_first(p))
        .and_then(|p| p.str("id"))
    })
    .unwrap_or_default();
  Ok(Context {
    fid: v.str("forum.id").unwrap_or_default(),
    forum: v.str("forum.name").unwrap_or_default(),
    first_pid,
  })
}

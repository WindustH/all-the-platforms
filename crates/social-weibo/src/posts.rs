//! One post and what hangs off it: the post (with its long text), comments
//! and their reply threads, who liked it, and its reposts.

use social_core::{Comment, Error, Page, PageReq, Post, Result, User, Value, ValueExt};

use crate::api::Client;
use crate::parse;
use crate::refs::{self, mid_to_bid};

/// The raw post, its long text filled in.
pub async fn show(c: &Client, arg: &str) -> Result<Value> {
  let mid = refs::mid(arg)?;
  let bid = mid_to_bid(&mid).unwrap_or_else(|| mid.clone());
  let mut v = c
    .get(
      "/ajax/statuses/show",
      &[("id", &bid), ("locale", "zh-CN"), ("isGetLongText", "true")],
    )
    .await?;
  if v.str("idstr").is_none() && v.str("mid").is_none() {
    return Err(Error::not_found(format!("no Weibo post {arg}")));
  }
  if v.bool("isLongText") == Some(true) && v.str("longText.longTextContent").is_none() {
    let long = c
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

pub async fn read(c: &Client, arg: &str) -> Result<Post> {
  let v = show(c, arg).await?;
  parse::post(&v).ok_or_else(|| Error::upstream("Weibo returned a post without an id"))
}

/// Comments, by heat (`hot`, the default) or by time (`time`).
pub async fn comments(
  c: &Client,
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
  let v = c.get("/ajax/statuses/buildComments", &query).await?;
  Ok(comment_page(&v))
}

/// Replies under one comment.
pub async fn replies(c: &Client, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
  let cid = comment.trim();
  if cid.is_empty() || !cid.bytes().all(|b| b.is_ascii_digit()) {
    return Err(Error::input(format!("not a Weibo comment id: {comment}")));
  }
  let max_id = page.cursor.clone().unwrap_or_else(|| "0".into());
  let count = page.size_within(20).to_string();
  let v = c
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
pub async fn likers(c: &Client, arg: &str, page: &PageReq) -> Result<Page<User>> {
  let mid = refs::mid(arg)?;
  let no = page.number_or(1);
  let count = page.size_within(20);
  let v = c
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
pub async fn reposts(c: &Client, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let mid = refs::mid(arg)?;
  let no = page.number_or(1);
  let v = c
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

//! Comments of songs, albums, playlists and episodes: hot, newest and
//! recommended pages, floor replies (楼层回复), commenting and deleting.

use atp_core::{Action, Comment, Error, Page, PageReq, Result, ValueExt};

use crate::api::Api;
use crate::parse;

/// `comments --sort` values; the first is the default.
pub const SORTS: &[&str] = &["hot", "new", "recommended"];

/// One page of a thread (`R_SO_4_ID` ...). The cursor is an offset for
/// `hot` and `recommended`, and the upstream time cursor for `new`.
pub async fn list(
  api: &Api,
  thread: &str,
  sort: Option<&str>,
  page: &PageReq,
) -> Result<Page<Comment>> {
  let size = page.size_within(50);
  let (sort_type, cursor, page_no) = match sort.unwrap_or(SORTS[0]) {
    "new" => (3, page.cursor.clone().unwrap_or_else(|| "0".into()), 1),
    sort => {
      let offset = page.number_or(0) as usize;
      let cursor = if sort == "hot" {
        format!("normalHot#{offset}")
      } else {
        offset.to_string()
      };
      (
        if sort == "hot" { 2 } else { 99 },
        cursor,
        offset / size + 1,
      )
    }
  };
  let v = api
    .client("v2/resource/comments")
    .arg("threadId", thread)
    .arg("pageNo", page_no)
    .arg("pageSize", size)
    .arg("cursor", cursor)
    .arg("sortType", sort_type)
    .arg("showInner", true)
    .send()
    .await?;
  let comments: Vec<Comment> = v
    .list("data.comments")
    .iter()
    .filter_map(parse::comment)
    .collect();
  let more = v.bool("data.hasMore").unwrap_or(false) && !comments.is_empty();
  let next = match sort_type {
    3 => v.str("data.cursor"),
    _ => Some((page.number_or(0) as usize + comments.len()).to_string()),
  };
  Ok(Page::new(comments, next.filter(|_| more)))
}

/// Replies under one comment (楼层), oldest first; the cursor is the
/// upstream time cursor.
pub async fn replies(
  api: &Api,
  thread: &str,
  comment: u64,
  page: &PageReq,
) -> Result<Page<Comment>> {
  let time: i64 = page
    .cursor
    .as_deref()
    .and_then(|c| c.parse().ok())
    .unwrap_or(-1);
  let v = api
    .web("resource/comment/floor/get")
    .arg("parentCommentId", comment)
    .arg("threadId", thread)
    .arg("time", time)
    .arg("limit", page.size_within(50))
    .send()
    .await?;
  let replies: Vec<Comment> = v
    .list("data.comments")
    .iter()
    .filter_map(parse::comment)
    .collect();
  let more = v.bool("data.hasMore").unwrap_or(false) && !replies.is_empty();
  let next = v.str("data.time").filter(|_| more);
  Ok(Page::new(replies, next))
}

/// Comment on a thread, or reply to one of its comments.
pub async fn add(api: &Api, thread: &str, text: &str, reply_to: Option<u64>) -> Result<Action> {
  if text.trim().is_empty() {
    return Err(Error::input("the comment is empty"));
  }
  let call = match reply_to {
    None => api.web("resource/comments/add"),
    Some(parent) => api.web("resource/comments/reply").arg("commentId", parent),
  };
  let v = call
    .arg("threadId", thread)
    .arg("content", text)
    .write()
    .send()
    .await?;
  let mut action = Action::done("comment", thread);
  if let Some(id) = v.str("comment.commentId") {
    action = action.with_id(id);
  }
  Ok(action)
}

/// Delete one of your comments (the shared command asked first).
pub async fn delete(api: &Api, thread: &str, comment: u64) -> Result<Action> {
  api
    .web("resource/comments/delete")
    .arg("threadId", thread)
    .arg("commentId", comment)
    .write()
    .send()
    .await?;
  Ok(Action::done("delete-comment", thread).with_id(comment.to_string()))
}

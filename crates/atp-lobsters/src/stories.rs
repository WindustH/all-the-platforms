//! Stories: the listings (hottest, newest, active, a tag's, a user's), one
//! story or comment, and its comment threads.

use atp_core::paging::slice;
use atp_core::{Comment, Page, PageReq, Post, Result, Value, ValueExt};

use crate::api::{self, Api, PAGE_SIZE};
use crate::parse;
use crate::refs::{self, ItemRef};

/// `hot --category`.
pub const HOT_CATEGORIES: &[&str] = &["hottest", "newest", "active"];

/// One page of a story listing (`hottest`, `t/rust`, `~name/stories`).
pub async fn listing(api: &Api, base: &str, page: &PageReq) -> Result<Page<Post>> {
  let no = page.number_or(1);
  let v = api.json(&api::listing(base, no)).await?;
  let rows = v.list("");
  let next = (rows.len() >= PAGE_SIZE).then(|| (no + 1).to_string());
  Ok(Page::new(rows.iter().map(parse::story).collect(), next))
}

pub async fn hot(api: &Api, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  listing(api, category.unwrap_or("hottest"), page).await
}

/// A story (its text and link; comments through `comments`), or a comment.
pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  match refs::item(arg)? {
    ItemRef::Story(id) => Ok(parse::story(&api.story(&id).await?)),
    ItemRef::Comment(id) => Ok(parse::comment_post(
      &api.json(&format!("c/{id}.json")).await?,
    )),
  }
}

/// A story's threads, as on its page.
async fn threads(api: &Api, arg: &str) -> Result<Vec<Comment>> {
  let story = api.story(&refs::story(arg)?).await?;
  Ok(parse::threads(story.list("comments")))
}

pub async fn comments(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Comment>> {
  Ok(slice(threads(api, arg).await?, page))
}

/// Replies under one comment of a story.
pub async fn replies(api: &Api, arg: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
  let id = match refs::item(comment)? {
    ItemRef::Comment(id) | ItemRef::Story(id) => id,
  };
  let all = threads(api, arg).await?;
  let replies = find(&all, &id)
    .map(|c| c.replies.clone())
    .unwrap_or_default();
  Ok(slice(replies, page))
}

fn find<'a>(comments: &'a [Comment], id: &str) -> Option<&'a Comment> {
  comments.iter().find_map(|comment| {
    if comment.id == id {
      Some(comment)
    } else {
      find(&comment.replies, id)
    }
  })
}

/// Every tag in use, as collections.
pub async fn tags(api: &Api, page: &PageReq) -> Result<Page<atp_core::Collection>> {
  let v: Value = api.json("tags.json").await?;
  let tags = v
    .list("")
    .iter()
    .filter(|tag| tag.bool("active") != Some(false))
    .map(parse::tag)
    .collect();
  Ok(slice(tags, page))
}

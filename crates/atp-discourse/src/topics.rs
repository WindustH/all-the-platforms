//! Reading topics: the forum's lists (latest, new, unread, hot, top), a
//! category's topics, the categories, one topic or post, the posts of a
//! topic and the replies to a post.

use atp_core::{Collection, Comment, Error, Page, PageReq, Post, Result, Value, ValueExt};

use crate::api::{Api, Query};
use crate::parse::{self, Forum};
use crate::refs::{self, CategoryRef, PostRef};

pub const FEED_KINDS: &[&str] = &["latest", "new", "unread", "unseen"];
/// The periods of `top`.
pub const PERIODS: &[&str] = &["daily", "weekly", "monthly", "quarterly", "yearly", "all"];
/// Posts per page of a topic.
const TOPIC_PAGE: usize = 20;

/// One page of a topic list (`page` counts from 0).
pub async fn list(api: &Api, path: &str, mut query: Query, page: &PageReq) -> Result<Page<Post>> {
  let number = page.number_or(0);
  if number > 0 {
    query.push(("page", number.to_string()));
  }
  let v = api.get(path, query).await?;
  let users = parse::users_by_id(&v);
  let base = api.forum();
  let forum = Forum { base: &base };
  let topics: Vec<Post> = v
    .list("topic_list.topics")
    .iter()
    .map(|t| forum.topic(t, &users))
    .collect();
  let more = v.str("topic_list.more_topics_url").is_some() && !topics.is_empty();
  Ok(Page::new(topics, more.then(|| (number + 1).to_string())))
}

/// `feed`: latest (the default), or your new, unread or unseen topics.
pub async fn feed(api: &Api, kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let kind = kind.unwrap_or("latest");
  if kind != "latest" {
    api.require()?;
  }
  if !FEED_KINDS.contains(&kind) {
    return Err(Error::input(format!("unknown list: {kind}")));
  }
  list(api, &format!("/{kind}.json"), Vec::new(), page).await
}

/// `hot`: the forum's hot list (Discourse 3.2 and later).
pub async fn hot(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  list(api, "/hot.json", Vec::new(), page).await
}

/// `top`: the most active topics of a period.
pub async fn top(api: &Api, period: &str, page: &PageReq) -> Result<Page<Post>> {
  let query = vec![("period", period.to_owned())];
  list(api, "/top.json", query, page).await
}

/// The topics of a category.
pub async fn category(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let path = match refs::category(arg)? {
    CategoryRef::Id(id) => format!("/c/{id}.json"),
    CategoryRef::Slug(slug) => format!("/c/{slug}.json"),
  };
  list(api, &path, Vec::new(), page).await
}

/// The topics with a tag.
pub async fn tagged(api: &Api, tag: &str, page: &PageReq) -> Result<Page<Post>> {
  let tag = tag.trim().trim_start_matches('#');
  let path = format!("/tag/{}.json", atp_core::text::encode_uri_component(tag));
  list(api, &path, Vec::new(), page).await
}

pub async fn categories(api: &Api) -> Result<Page<Collection>> {
  let v = api.get("/categories.json", Vec::new()).await?;
  let base = api.forum();
  let forum = Forum { base: &base };
  Ok(Page::last(
    v.list("category_list.categories")
      .iter()
      .map(|c| forum.category(c))
      .collect(),
  ))
}

/// The post a reference points at (`/posts/<id>.json`).
pub async fn post_json(api: &Api, post: &PostRef) -> Result<Value> {
  match post {
    PostRef::Post(id) => api.get(&format!("/posts/{id}.json"), Vec::new()).await,
    PostRef::Topic { id, number } => {
      let number = number.unwrap_or(1);
      api
        .get(&format!("/posts/by_number/{id}/{number}.json"), Vec::new())
        .await
    }
  }
}

/// A topic (with its first post as the text), or one post of it.
pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let base = api.forum();
  let forum = Forum { base: &base };
  match refs::post(arg)? {
    PostRef::Topic { id, number: None } => {
      let v = api.get(&format!("/t/{id}.json"), Vec::new()).await?;
      Ok(forum.topic_page(&v))
    }
    post => Ok(forum.post(&post_json(api, &post).await?, None)),
  }
}

/// The topic a post argument belongs to.
async fn topic_of(api: &Api, arg: &str) -> Result<u64> {
  match refs::post(arg)? {
    PostRef::Topic { id, .. } => Ok(id),
    post => post_json(api, &post)
      .await?
      .u64("topic_id")
      .ok_or_else(|| Error::not_found(format!("no topic for {arg}"))),
  }
}

/// The posts after the first one, twenty a page (`page` counts from 1).
pub async fn comments(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Comment>> {
  let id = topic_of(api, arg).await?;
  let number = page.number_or(1).max(1);
  let query = vec![("page", number.to_string())];
  let v = api.get(&format!("/t/{id}.json"), query).await?;
  let base = api.forum();
  let forum = Forum { base: &base };
  let comments: Vec<Comment> = v
    .list("post_stream.posts")
    .iter()
    .filter(|p| p.u64("post_number").unwrap_or(1) > 1)
    .map(|p| forum.comment(p))
    .collect();
  let total = v.list("post_stream.stream").len();
  let more = (number as usize) * TOPIC_PAGE < total;
  Ok(Page::new(comments, more.then(|| (number + 1).to_string())))
}

/// The direct replies to one post (`/posts/<id>/replies.json`).
pub async fn replies(api: &Api, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
  let post = match refs::post(comment)? {
    PostRef::Topic { number: None, id } => PostRef::Post(id),
    other => other,
  };
  let id = match post {
    PostRef::Post(id) => id,
    topic => post_json(api, &topic)
      .await?
      .u64("id")
      .ok_or_else(|| Error::not_found(format!("no post {comment}")))?,
  };
  let v = api
    .get(&format!("/posts/{id}/replies.json"), Vec::new())
    .await?;
  let base = api.forum();
  let forum = Forum { base: &base };
  let rows = v.as_array().map(Vec::as_slice).unwrap_or_default();
  Ok(atp_core::paging::slice(
    rows.iter().map(|p| forum.comment(p)).collect(),
    page,
  ))
}

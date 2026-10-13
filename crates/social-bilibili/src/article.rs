//! Column articles (专栏, `cv…`): reading, liking and saving them. Their
//! comment section is `(cv id, 12)`.

use social_core::text::{from_secs, html_to_text};
use social_core::{Action, Ctx, Media, Metrics, Post, Result, Value, ValueExt, json};

use crate::refs::{self, article_url};
use crate::{api, parse};

const VIEW: &str = "https://api.bilibili.com/x/article/view";
const LIKE: &str = "https://api.bilibili.com/x/article/like";
const FAV_ADD: &str = "https://api.bilibili.com/x/article/favorites/add";
const FAV_DEL: &str = "https://api.bilibili.com/x/article/favorites/del";

/// Comment section type of articles.
pub const COMMENT_TYPE: u64 = 12;

pub async fn view(ctx: &Ctx, id: u64) -> Result<Value> {
  api::get(ctx, VIEW)
    .arg("id", id)
    .arg("gaia_source", "main_web")
    .send()
    .await
}

pub async fn read(ctx: &Ctx, id: u64) -> Result<Post> {
  Ok(post(id, view(ctx, id).await?))
}

/// The article behind a `view` answer (also the shape of search results).
pub fn post(id: u64, v: Value) -> Post {
  let mut metrics = Metrics {
    views: v.first_count(&["stats.view", "view"]),
    likes: v.first_count(&["stats.like", "like"]),
    comments: v.first_count(&["stats.reply", "reply"]),
    shares: v.count("stats.share"),
    favorites: v.count("stats.favorite"),
    ..Metrics::default()
  };
  if let Some(n) = v.count("stats.coin") {
    metrics.other.insert("coins".into(), n);
  }
  let images: Vec<Media> = v
    .list("image_urls")
    .iter()
    .chain(v.list("origin_image_urls"))
    .filter_map(|u| u.as_str())
    .map(|u| Media::image(parse::https(u.to_owned())))
    .fold(Vec::new(), |mut acc, m| {
      if !acc.iter().any(|a: &Media| a.url == m.url) {
        acc.push(m);
      }
      acc
    });
  let mut post = Post {
    id: format!("cv{id}"),
    kind: "article".into(),
    title: parse::plain(&v, &["title"]),
    text: v
      .str("content")
      .map(|c| html_to_text(&c))
      .or_else(|| v.str("summary"))
      .filter(|t| !t.is_empty()),
    url: Some(article_url(id)),
    author: Some(parse::user(v.at("author"))).filter(|u| !u.id.is_empty()),
    created_at: v
      .first_str(&["publish_time", "pub_time", "ctime"])
      .and_then(|t| t.parse().ok())
      .and_then(from_secs),
    metrics,
    media: images,
    tags: v
      .list("tags")
      .iter()
      .filter_map(|t| t.str("name"))
      .collect(),
    raw: Some(v.clone()),
    ..Post::default()
  };
  for (key, path) in [
    ("words", "words"),
    ("category", "category.name"),
    ("dynamic_id", "dyn_id_str"),
    ("list", "list.name"),
  ] {
    if let Some(x) = v.str(path) {
      post.extra.insert(key.into(), json!(x));
    }
  }
  post
}

pub async fn like(ctx: &Ctx, id: u64, undo: bool) -> Result<Action> {
  api::post(ctx, LIKE)
    .arg("id", id)
    .arg("type", if undo { 2 } else { 1 })
    .send()
    .await?;
  let action = if undo { "unlike" } else { "like" };
  Ok(Action::done(action, format!("cv{id}")).with_url(article_url(id)))
}

pub async fn favorite(ctx: &Ctx, id: u64, undo: bool) -> Result<Action> {
  api::post(ctx, if undo { FAV_DEL } else { FAV_ADD })
    .arg("id", id)
    .send()
    .await?;
  let action = if undo { "unfavorite" } else { "favorite" };
  Ok(Action::done(action, format!("cv{id}")).with_url(refs::article_url(id)))
}

//! The material library (素材): permanent images, voice, video and legacy
//! news items through the API (every account type), or the backend's image
//! library when only a QR session exists.

use std::path::Path;

use social_core::text::from_secs;
use social_core::{Action, Error, Media, Page, PageReq, Post, Result, Value, ValueExt, json};

use crate::client::Client;
use crate::upload;

pub const KINDS: &[&str] = &["image", "video", "voice", "news"];

pub async fn list(c: &Client, kind: &str, req: &PageReq) -> Result<Page<Post>> {
  c.require_any()?;
  if !c.has_api() {
    if kind != "image" {
      return Err(Error::unsupported(
        "material other than images without API credentials",
      ));
    }
    return web_images(c, req).await;
  }
  let offset = req.number_or(0);
  let count = req.size_within(20);
  let v = c
    .api_post(
      "/cgi-bin/material/batchget_material",
      &json!({ "type": kind, "offset": offset, "count": count }),
    )
    .await?;
  let posts = v
    .list("item")
    .iter()
    .map(|m| {
      let news = m.list("content.news_item");
      let mut post = Post {
        id: m.str("media_id").unwrap_or_default(),
        kind: kind.into(),
        title: m
          .str("name")
          .or_else(|| news.first().and_then(|n| n.str("title"))),
        url: m
          .str("url")
          .or_else(|| news.first().and_then(|n| n.str("url"))),
        created_at: m.i64("update_time").and_then(from_secs),
        raw: Some(m.clone()),
        ..Post::default()
      };
      if kind == "image"
        && let Some(url) = &post.url
      {
        post.media.push(Media::image(url.clone()));
      }
      post
    })
    .collect();
  let total = v.u64("total_count").unwrap_or(0);
  let next = (offset + (count as u64) < total).then(|| (offset + count as u64).to_string());
  Ok(Page::new(posts, next))
}

async fn web_images(c: &Client, req: &PageReq) -> Result<Page<Post>> {
  let begin = req.number_or(0);
  let count = req.size_within(20);
  let v = c
    .web_get(
      "/cgi-bin/filepage",
      vec![
        ("type", "2".into()),
        ("begin", begin.to_string()),
        ("count", count.to_string()),
        ("group_id", String::new()),
      ],
    )
    .await?;
  let items = v.list("page_info.file_item");
  let posts = items
    .iter()
    .map(|f| {
      let url = f.str("cdn_url");
      Post {
        id: f.str("file_id").unwrap_or_default(),
        kind: "image".into(),
        title: f.str("name"),
        url: url.clone(),
        created_at: f.i64("update_time").and_then(from_secs),
        media: url.into_iter().map(Media::image).collect(),
        raw: Some(f.clone()),
        ..Post::default()
      }
    })
    .collect();
  let next = (items.len() >= count).then(|| (begin + count as u64).to_string());
  Ok(Page::new(posts, next))
}

pub async fn count(c: &Client) -> Result<Value> {
  let v = c
    .api_get("/cgi-bin/material/get_materialcount", &[])
    .await?;
  Ok(json!({
    "image": v.u64("image_count"),
    "video": v.u64("video_count"),
    "voice": v.u64("voice_count"),
    "news": v.u64("news_count"),
  }))
}

pub async fn add(
  c: &Client,
  path: &Path,
  kind: &str,
  title: Option<&str>,
  intro: Option<&str>,
) -> Result<Action> {
  if !path.is_file() {
    return Err(Error::input(format!("file not found: {}", path.display())));
  }
  let video = (kind == "video").then(|| {
    (
      title.unwrap_or_else(|| path.file_stem().and_then(|s| s.to_str()).unwrap_or("video")),
      intro.unwrap_or_default(),
    )
  });
  let (id, url) = upload::api_material(c, path, kind, video).await?;
  let mut action = Action::done("upload", path.display().to_string()).with_id(id);
  if let Some(u) = url {
    action = action.with_url(u);
  }
  Ok(action)
}

pub async fn delete(c: &Client, id: &str) -> Result<Action> {
  c.api_post("/cgi-bin/material/del_material", &json!({ "media_id": id }))
    .await?;
  Ok(Action::done("delete-material", id))
}

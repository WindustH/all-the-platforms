//! Drafts (草稿箱), the one write path every account type has.
//!
//! Through the API a draft is a `media_id`; in the backend it is a numeric
//! `appMsgId`. Ids are routed by their shape, listings by what is available
//! (the API when credentials are set). The API demands a cover image
//! (`thumb_media_id`); the backend saves drafts without one.

use std::path::PathBuf;

use social_core::text::{from_secs, html_to_text};
use social_core::{Action, Error, Page, PageReq, Post, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::backend::{Params, nested};
use crate::body::Body;
use crate::{uploads, users};

/// What a new draft holds.
#[derive(Debug, Clone, Default)]
pub struct Compose {
  pub title: String,
  pub text: String,
  pub images: Vec<PathBuf>,
  pub cover: Option<PathBuf>,
  pub author: Option<String>,
  pub digest: Option<String>,
  pub source_url: Option<String>,
  pub no_comments: bool,
  pub fans_only_comments: bool,
}

/// A backend draft id is all digits; anything else is an API `media_id`.
pub fn is_web_id(id: &str) -> bool {
  !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())
}

pub async fn create(api: &Api, d: &Compose) -> Result<Action> {
  api.require_any()?;
  if d.title.trim().is_empty() {
    return Err(Error::input("a draft needs a --title"));
  }
  let base = std::env::current_dir()?;
  let body = Body::new(&d.text, &d.images, &base)?;
  let id = if api.official().has() {
    api_create(api, d, &body).await?
  } else {
    web_create(api, d, &body).await?
  };
  Ok(
    Action::done("draft", d.title.trim())
      .with_id(&id)
      .with_message(format!(
        "saved to the drafts; publish it with `social mp publish {id}`"
      )),
  )
}

async fn api_create(api: &Api, d: &Compose, body: &Body) -> Result<String> {
  if d.title.chars().count() > 32 {
    return Err(Error::input(
      "API drafts take titles of at most 32 characters",
    ));
  }
  let cover = d
    .cover
    .clone()
    .or_else(|| body.images.first().cloned())
    .ok_or_else(|| {
      Error::input("a draft through the API needs a cover: attach an image (-i) or give --cover")
    })?;
  let mut urls = Vec::new();
  for path in &body.images {
    urls.push(uploads::api_body_image(api, path).await?);
  }
  let (thumb, _) = uploads::api_material(api, &cover, "image", None).await?;
  let mut article = json!({
    "article_type": "news",
    "title": d.title.trim(),
    "content": body.fill(&urls),
    "thumb_media_id": thumb,
    "need_open_comment": u8::from(!d.no_comments),
    "only_fans_can_comment": u8::from(d.fans_only_comments),
  });
  for (key, value) in [
    ("author", &d.author),
    ("digest", &d.digest),
    ("content_source_url", &d.source_url),
  ] {
    if let Some(v) = value {
      article[key] = json!(v);
    }
  }
  let v = api
    .official()
    .post("/cgi-bin/draft/add", &json!({ "articles": [article] }))
    .await?;
  v.str("media_id")
    .ok_or_else(|| Error::upstream("draft/add returned no media_id"))
}

async fn web_create(api: &Api, d: &Compose, body: &Body) -> Result<String> {
  let home = users::home(api).await?;
  let mut urls = Vec::new();
  let mut files = Vec::new();
  for path in &body.images {
    let (file, url) = uploads::web_image(api, &home, path, "8").await?;
    files.push(file);
    urls.push(url);
  }
  let cover = match &d.cover {
    Some(path) => uploads::web_image(api, &home, path, "8").await?,
    None => (
      files.first().cloned().unwrap_or_default(),
      urls.first().cloned().unwrap_or_default(),
    ),
  };
  let query = vec![
    ("t", "ajax-response".into()),
    ("sub", "create".into()),
    ("type", "77".into()),
  ];
  let form = web_form(d, body.fill(&urls), cover);
  let v = api
    .backend()
    .post("/cgi-bin/operate_appmsg", query, form)
    .await?;
  let id = v
    .str("appMsgId")
    .ok_or_else(|| Error::upstream(format!("the backend saved no draft: {v}")))?;
  // `ret` 0 alone has not always meant a visible draft: look for it.
  let first = PageReq {
    cursor: None,
    size: 10,
  };
  let listed = web_list(api, &first)
    .await
    .map(|p| p.items.iter().any(|d| d.id == id))
    .unwrap_or(true);
  if !listed {
    return Err(Error::upstream(format!(
      "the backend answered with draft {id}, but it is not in the draft box"
    )));
  }
  Ok(id)
}

/// The editor's `operate_appmsg` form for one article; `cover` is the
/// uploaded cover's `(file id, CDN URL)`.
fn web_form(d: &Compose, content: String, cover: (String, String)) -> Params {
  let flag = |on: bool| if on { "1" } else { "0" }.to_owned();
  vec![
    ("AppMsgId", String::new()),
    ("count", "1".into()),
    ("data_seq", "0".into()),
    ("operate_from", "Chrome".into()),
    ("isnew", "0".into()),
    ("title0", d.title.trim().to_owned()),
    ("author0", d.author.clone().unwrap_or_default()),
    ("writerid0", "0".into()),
    ("digest0", d.digest.clone().unwrap_or_default()),
    ("auto_gen_digest0", flag(d.digest.is_none())),
    ("content0", content),
    ("sourceurl0", d.source_url.clone().unwrap_or_default()),
    ("need_open_comment0", flag(!d.no_comments)),
    ("only_fans_can_comment0", flag(d.fans_only_comments)),
    ("fileid0", cover.0),
    ("cdn_url0", cover.1),
    ("show_cover_pic0", "0".into()),
    ("copyright_type0", "0".into()),
    ("random", format!("{}", rand::random::<f64>())),
  ]
}

pub async fn list(api: &Api, req: &PageReq) -> Result<Page<Post>> {
  api.require_any()?;
  if api.official().has() {
    api_list(api, req).await
  } else {
    web_list(api, req).await
  }
}

async fn api_list(api: &Api, req: &PageReq) -> Result<Page<Post>> {
  let offset = req.number_or(0);
  let count = req.size_within(20);
  let v = api
    .official()
    .post(
      "/cgi-bin/draft/batchget",
      &json!({ "offset": offset, "count": count, "no_content": 1 }),
    )
    .await?;
  let posts = v
    .list("item")
    .iter()
    .map(|item| {
      api_draft(
        item.str("media_id").unwrap_or_default(),
        item.list("content.news_item"),
        item.i64("update_time"),
      )
    })
    .collect();
  let total = v.u64("total_count").unwrap_or(0);
  let next = (offset + (count as u64) < total).then(|| (offset + count as u64).to_string());
  Ok(Page::new(posts, next))
}

fn api_draft(id: String, articles: &[Value], updated: Option<i64>) -> Post {
  let first = articles.first().cloned().unwrap_or_default();
  let mut post = Post {
    id,
    kind: "draft".into(),
    title: first.str("title"),
    text: first
      .str("content")
      .map(|h| html_to_text(&h))
      .or_else(|| first.str("digest")),
    url: first.str("url"),
    created_at: updated.and_then(from_secs),
    raw: Some(Value::from(articles.to_vec())),
    ..Post::default()
  };
  articles_extra(&mut post, articles, "author");
  post
}

fn articles_extra(post: &mut Post, articles: &[Value], author_key: &str) {
  let titles: Vec<Value> = articles
    .iter()
    .filter_map(|a| a.str("title").map(Value::from))
    .collect();
  if titles.len() > 1 {
    post.extra.insert("articles".into(), Value::from(titles));
  }
  if let Some(first) = articles.first() {
    for (key, path) in [("writer", author_key), ("digest", "digest")] {
      if let Some(v) = first.str(path) {
        post.extra.insert(key.into(), v.into());
      }
    }
  }
}

async fn web_list(api: &Api, req: &PageReq) -> Result<Page<Post>> {
  api.backend().require()?;
  let begin = req.number_or(0);
  let count = req.size_within(20);
  let v = api
    .backend()
    .get(
      "/cgi-bin/appmsg",
      vec![
        ("action", "list_card".into()),
        ("type", "77".into()),
        ("begin", begin.to_string()),
        ("count", count.to_string()),
        ("query", String::new()),
      ],
    )
    .await?;
  let info = nested(&v, "app_msg_info");
  let items = info.list("item");
  let posts = items.iter().map(web_draft).collect();
  let total = info.u64("file_cnt.draft_count").unwrap_or(0);
  let next = (items.len() >= count && (total == 0 || begin + (count as u64) < total))
    .then(|| (begin + count as u64).to_string());
  Ok(Page::new(posts, next))
}

fn web_draft(item: &Value) -> Post {
  let articles = item.list("multi_item");
  let first = articles.first().cloned().unwrap_or_else(|| item.clone());
  let mut post = Post {
    id: item
      .str("app_id")
      .or_else(|| item.str("appmsgid"))
      .unwrap_or_default(),
    kind: "draft".into(),
    title: first.str("title").or_else(|| item.str("title")),
    text: first
      .str("content")
      .map(|h| html_to_text(&h))
      .or_else(|| first.str("digest")),
    created_at: item
      .i64("update_time")
      .or_else(|| item.i64("create_time"))
      .and_then(from_secs),
    raw: Some(item.clone()),
    ..Post::default()
  };
  articles_extra(&mut post, articles, "author");
  post
}

pub async fn get(api: &Api, id: &str) -> Result<Post> {
  if is_web_id(id) {
    let v = api
      .backend()
      .get(
        "/cgi-bin/appmsg",
        vec![
          ("t", "media/appmsg_edit".into()),
          ("action", "edit".into()),
          ("type", "77".into()),
          ("appmsgid", id.into()),
          ("isMul", "1".into()),
          ("replaceScene", "0".into()),
          ("isSend", "0".into()),
          ("isFreePublish", "0".into()),
          (
            "timestamp",
            jiff::Timestamp::now().as_millisecond().to_string(),
          ),
        ],
      )
      .await?;
    let info = nested(&v, "app_msg_info");
    let item = info
      .list("item")
      .first()
      .cloned()
      .ok_or_else(|| Error::not_found(format!("no draft {id}")))?;
    let mut post = web_draft(&item);
    post.id = id.into();
    Ok(post)
  } else {
    let v = api
      .official()
      .post("/cgi-bin/draft/get", &json!({ "media_id": id }))
      .await?;
    Ok(api_draft(id.into(), v.list("news_item"), None))
  }
}

pub async fn delete(api: &Api, id: &str) -> Result<Action> {
  if is_web_id(id) {
    api
      .backend()
      .post(
        "/cgi-bin/operate_appmsg",
        vec![("sub", "del".into()), ("t", "ajax-response".into())],
        vec![("AppMsgId", id.into())],
      )
      .await?;
  } else {
    api
      .official()
      .post("/cgi-bin/draft/delete", &json!({ "media_id": id }))
      .await?;
  }
  Ok(Action::done("delete-draft", id))
}

/// Number of drafts (API).
pub async fn count(api: &Api) -> Result<u64> {
  let v = api.official().get("/cgi-bin/draft/count", &[]).await?;
  Ok(v.u64("total_count").unwrap_or(0))
}

/// Articles in a backend draft (for publishing a multi-article group).
pub async fn web_article_count(api: &Api, id: &str) -> Result<usize> {
  let post = get(api, id).await?;
  Ok(
    post
      .extra
      .get("articles")
      .and_then(Value::as_array)
      .map_or(1, Vec::len),
  )
}

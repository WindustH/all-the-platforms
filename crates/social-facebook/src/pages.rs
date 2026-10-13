//! Facebook Pages through the Graph API (a token from `login --token` or
//! FACEBOOK_ACCESS_TOKEN): the Pages a token manages, their posts,
//! publishing, comments, Page and post insights, and Messenger
//! conversations. Writes use the Page's own token, looked up per call.
//!
//! The Page to act on: `--topic PAGE` / an id in the argument,
//! FACEBOOK_PAGE_ID, the one saved by `use-page`, or the only one managed.

use std::path::Path;

use jiff::{Timestamp, ToSpan};
use social_core::file::Image;
use social_core::http::Part;
use social_core::text::parse_time;
use social_core::{
  Action, Breakdown, Collection, Comment, Draft, Error, Insights, Media, Message, Metrics, Page,
  PageReq, Point, Post, Reply, Result, Series, Share, User, Value, ValueExt, json,
};
use social_meta::Graph;
use social_meta::graph::Params;

use crate::client::Client;

const PAGE_KEY: &str = "page";
const PAGE_FIELDS: &str = "id,name,category,fan_count,followers_count,link,username,tasks";
const POST_FIELDS: &str = "id,message,story,created_time,updated_time,permalink_url,full_picture,status_type,from{id,name},attachments{media_type,media,url,title,subattachments},shares,reactions.summary(total_count).limit(0),comments.summary(total_count).limit(0)";
/// Page metrics, asked one by one: Meta retires them every few releases and
/// one unknown name fails the whole request.
const PAGE_METRICS: &[&str] = &[
  "page_media_view",
  "page_total_media_view_unique",
  "page_impressions",
  "page_impressions_unique",
  "page_post_engagements",
  "page_views_total",
  "page_total_actions",
  "page_follows",
  "page_daily_follows_unique",
  "page_daily_unfollows_unique",
  "page_video_views",
];
const POST_METRICS: &[&str] = &[
  "post_media_view",
  "post_total_media_view_unique",
  "post_impressions",
  "post_impressions_unique",
  "post_clicks",
  "post_reactions_by_type_total",
  "post_video_views",
];

fn s(v: &str) -> String {
  v.to_owned()
}

/// The Pages the token manages.
pub async fn pages(c: &Client) -> Result<Vec<Collection>> {
  let g = c.graph()?;
  let items = match g
    .all("me/accounts", &[(s("fields"), s(PAGE_FIELDS))], 0)
    .await
  {
    Ok(items) => items,
    // A Page token: `me` is the Page itself.
    Err(_) => vec![g.get("me", &[(s("fields"), s(PAGE_FIELDS))]).await?],
  };
  Ok(items.iter().map(page_collection).collect())
}

fn page_collection(p: &Value) -> Collection {
  Collection {
    id: p.str("id").unwrap_or_default(),
    kind: "page".into(),
    name: p.str("name").unwrap_or_default(),
    description: p.str("category"),
    url: p.str("link"),
    followers: p.count("followers_count").or_else(|| p.count("fan_count")),
    extra: [
      ("username", p.at("username").clone()),
      ("likes", p.at("fan_count").clone()),
      ("tasks", p.at("tasks").clone()),
    ]
    .into_iter()
    .filter(|(_, v)| !v.is_null())
    .map(|(k, v)| (k.to_owned(), v))
    .collect(),
    ..Collection::default()
  }
}

/// The Page to work on: given, FACEBOOK_PAGE_ID, saved, or the only one.
pub async fn page_id(c: &Client, given: Option<&str>) -> Result<String> {
  if let Some(p) = given
    .map(str::to_owned)
    .or_else(|| {
      std::env::var("FACEBOOK_PAGE_ID")
        .ok()
        .filter(|v| !v.trim().is_empty())
    })
    .or_else(|| c.ctx.extra(PAGE_KEY))
  {
    return Ok(p.trim().to_owned());
  }
  let all = pages(c).await?;
  match all.as_slice() {
    [one] => Ok(one.id.clone()),
    [] => Err(Error::not_found("the token manages no Facebook Page")),
    _ => Err(
      Error::input("which Page? pass it (e.g. --topic PAGE_ID), set FACEBOOK_PAGE_ID or run `social facebook use-page ID`")
        .with_hint("`social facebook pages` lists the Pages your token manages"),
    ),
  }
}

pub async fn use_page(c: &Client, page: &str) -> Result<Action> {
  let g = c.graph()?;
  let v = g.get(page, &[(s("fields"), s("id,name"))]).await?;
  let id = v.str("id").unwrap_or_else(|| page.to_owned());
  c.ctx.set_extra(PAGE_KEY, &id);
  Ok(
    Action::done("use-page", v.str("name").unwrap_or_else(|| id.clone()))
      .with_id(id)
      .with_message("used by default from now on"),
  )
}

/// The Graph API acting as a Page (its token, via the user token).
async fn as_page<'a>(c: &'a Client, page: &str) -> Result<Graph<'a>> {
  let g = c.graph()?;
  match g.get(page, &[(s("fields"), s("access_token"))]).await {
    Ok(v) => match v.str("access_token") {
      Some(t) => Ok(g.with_token(t)),
      None => Ok(g),
    },
    // Already a Page token, or no rights to read it: try with what we have.
    Err(_) => Ok(g),
  }
}

/// The Page part of a Graph post or comment id (`PAGE_POST`, `PAGE_POST_COMMENT`).
fn owner_of(id: &str) -> Option<&str> {
  id.split_once('_').map(|(p, _)| p)
}

/// A Graph object id from an argument: `123_456` / `123` as is.
pub fn graph_id(arg: &str) -> Result<String> {
  let a = arg.trim();
  if !a.is_empty() && a.bytes().all(|b| b.is_ascii_digit() || b == b'_') {
    return Ok(a.to_owned());
  }
  Err(
    Error::input(format!("`{arg}` is not a Graph API id"))
      .with_hint("Page posts and comments are written by Graph id (PAGE_POST); `social facebook page-posts` lists them"),
  )
}

pub fn post(v: &Value) -> Post {
  let mut media = Vec::new();
  for a in v.list("attachments.data") {
    collect_media(a, &mut media);
  }
  let mut p = Post {
    id: v.str("id").unwrap_or_default(),
    kind: v.str("status_type").unwrap_or_else(|| "post".into()),
    text: v.str("message").or_else(|| v.str("story")),
    url: v.str("permalink_url"),
    author: v.at("from").is_object().then(|| User {
      id: v.str("from.id").unwrap_or_default(),
      name: v.str("from.name").unwrap_or_default(),
      ..User::default()
    }),
    created_at: v.str("created_time").and_then(|t| graph_time(&t)),
    updated_at: v.str("updated_time").and_then(|t| graph_time(&t)),
    metrics: Metrics {
      likes: v.count("reactions.summary.total_count"),
      comments: v.count("comments.summary.total_count"),
      shares: v.count("shares.count"),
      ..Metrics::default()
    },
    media,
    raw: Some(v.clone()),
    ..Post::default()
  };
  if let Some(pic) = v.str("full_picture") {
    p.extra.insert("picture".into(), pic.into());
  }
  p
}

fn collect_media(a: &Value, out: &mut Vec<Media>) {
  match a.str("media_type").as_deref() {
    Some("video") => {
      if let Some(src) = a.str("media.source") {
        out.push(Media::video(src));
      }
    }
    _ => {
      if let Some(src) = a.str("media.image.src") {
        out.push(Media::image(src));
      }
    }
  }
  for sub in a.list("subattachments.data") {
    collect_media(sub, out);
  }
}

/// Graph times (`2026-10-13T09:00:00+0000`) as timestamps.
fn graph_time(t: &str) -> Option<Timestamp> {
  parse_time(t).or_else(|| {
    jiff::fmt::strtime::parse("%Y-%m-%dT%H:%M:%S%z", t)
      .ok()?
      .to_timestamp()
      .ok()
  })
}

pub async fn page_posts(c: &Client, page: Option<&str>, req: &PageReq) -> Result<Page<Post>> {
  let page = page_id(c, page).await?;
  let g = as_page(c, &page).await?;
  let (items, next) = g
    .page(
      &format!("{page}/posts"),
      &[(s("fields"), s(POST_FIELDS))],
      req,
      100,
    )
    .await?;
  Ok(Page::new(items.iter().map(post).collect(), next))
}

pub async fn read(c: &Client, id: &str) -> Result<Post> {
  let id = graph_id(id)?;
  let g = match owner_of(&id) {
    Some(owner) => as_page(c, owner).await?,
    None => c.graph()?,
  };
  Ok(post(&g.get(&id, &[(s("fields"), s(POST_FIELDS))]).await?))
}

/// Publish to a Page: text (`message`), one photo, or several photos
/// uploaded unpublished and attached to one post.
pub async fn publish(c: &Client, draft: &Draft) -> Result<Action> {
  if draft.reply_to.is_some() || draft.quote.is_some() {
    return Err(Error::unsupported(
      "post --reply-to / --quote (use `comment` on Pages)",
    ));
  }
  let page = page_id(c, draft.topics.first().map(String::as_str)).await?;
  let g = as_page(c, &page).await?;
  let mut message = draft.text.clone();
  if let Some(t) = &draft.title {
    message = if message.is_empty() {
      t.clone()
    } else {
      format!("{t}\n\n{message}")
    };
  }
  let v = match draft.images.as_slice() {
    [] => {
      g.post(&format!("{page}/feed"), &[(s("message"), message)])
        .await?
    }
    [one] => {
      let parts = vec![photo_part(one).await?, Part::text("message", message)];
      g.post_multipart(&format!("{page}/photos"), parts).await?
    }
    many => {
      let mut form: Params = vec![(s("message"), message)];
      for (i, path) in many.iter().enumerate() {
        let parts = vec![photo_part(path).await?, Part::text("published", "false")];
        let up = g.post_multipart(&format!("{page}/photos"), parts).await?;
        let id = up
          .str("id")
          .ok_or_else(|| Error::upstream("photo upload returned no id"))?;
        form.push((
          format!("attached_media[{i}]"),
          json!({"media_fbid": id}).to_string(),
        ));
      }
      g.post(&format!("{page}/feed"), &form).await?
    }
  };
  let id = v.str("post_id").or_else(|| v.str("id")).unwrap_or_default();
  Ok(
    Action::done("publish", &page)
      .with_url(format!("https://www.facebook.com/{id}"))
      .with_id(id),
  )
}

async fn photo_part(path: &Path) -> Result<Part> {
  let img = Image::read(path).await?;
  Ok(Part::file("source", img.data, &img.name, img.mime))
}

pub async fn delete(c: &Client, id: &str) -> Result<Action> {
  let id = graph_id(id)?;
  let g = match owner_of(&id) {
    Some(owner) => as_page(c, owner).await?,
    None => c.graph()?,
  };
  g.delete(&id).await?;
  Ok(Action::done("delete", &id))
}

pub async fn comments(c: &Client, id: &str, req: &PageReq) -> Result<Page<Comment>> {
  let id = graph_id(id)?;
  let g = match owner_of(&id) {
    Some(owner) => as_page(c, owner).await?,
    None => c.graph()?,
  };
  let (items, next) = g
    .page(
      &format!("{id}/comments"),
      &[
        (
          s("fields"),
          s("id,message,created_time,from{id,name},like_count,comment_count,parent{id}"),
        ),
        (s("order"), s("reverse_chronological")),
        (s("filter"), s("toplevel")),
      ],
      req,
      100,
    )
    .await?;
  Ok(Page::new(items.iter().map(comment).collect(), next))
}

fn comment(v: &Value) -> Comment {
  Comment {
    id: v.str("id").unwrap_or_default(),
    author: v.at("from").is_object().then(|| User {
      id: v.str("from.id").unwrap_or_default(),
      name: v.str("from.name").unwrap_or_default(),
      ..User::default()
    }),
    text: v.str("message").unwrap_or_default(),
    created_at: v.str("created_time").and_then(|t| graph_time(&t)),
    likes: v.count("like_count"),
    reply_count: v.count("comment_count"),
    raw: Some(v.clone()),
    ..Comment::default()
  }
}

pub async fn replies(c: &Client, comment_id: &str, req: &PageReq) -> Result<Page<Comment>> {
  let id = graph_id(comment_id)?;
  let g = match owner_of(&id) {
    Some(owner) => as_page(c, owner).await?,
    None => c.graph()?,
  };
  let (items, next) = g
    .page(
      &format!("{id}/comments"),
      &[(
        s("fields"),
        s("id,message,created_time,from{id,name},like_count,comment_count"),
      )],
      req,
      100,
    )
    .await?;
  Ok(Page::new(items.iter().map(comment).collect(), next))
}

pub async fn comment_on(c: &Client, post_arg: &str, reply: &Reply) -> Result<Action> {
  let post = graph_id(post_arg)?;
  let target = match &reply.reply_to {
    Some(r) => graph_id(r)?,
    None => post.clone(),
  };
  let g = match owner_of(&post) {
    Some(owner) => as_page(c, owner).await?,
    None => c.graph()?,
  };
  let v = match reply.images.as_slice() {
    [] => {
      g.post(
        &format!("{target}/comments"),
        &[(s("message"), reply.text.clone())],
      )
      .await?
    }
    [one, ..] => {
      let parts = vec![
        photo_part(one).await?,
        Part::text("message", reply.text.clone()),
      ];
      g.post_multipart(&format!("{target}/comments"), parts)
        .await?
    }
  };
  Ok(Action::done("comment", &target).with_id(v.str("id").unwrap_or_default()))
}

pub async fn delete_comment(c: &Client, post_arg: &str, comment: &str) -> Result<Action> {
  let post = graph_id(post_arg)?;
  let comment = graph_id(comment)?;
  let g = match owner_of(&post) {
    Some(owner) => as_page(c, owner).await?,
    None => c.graph()?,
  };
  g.delete(&comment).await?;
  Ok(Action::done("delete-comment", &comment))
}

// ── insights ───────────────────────────────────────────────────────────────

/// Page insights day by day over `days`, or lifetime numbers of one post.
pub async fn insights(c: &Client, post_arg: Option<&str>, days: u32) -> Result<Insights> {
  match post_arg {
    Some(p) => post_insights(c, p).await,
    None => page_insights(c, days).await,
  }
}

async fn page_insights(c: &Client, days: u32) -> Result<Insights> {
  let page = page_id(c, None).await?;
  let g = as_page(c, &page).await?;
  let info = g
    .get(
      &page,
      &[(s("fields"), s("name,link,followers_count,fan_count"))],
    )
    .await?;
  let days = days.clamp(1, 90);
  let until = Timestamp::now();
  let since = until - (i64::from(days) * 24).hours();
  let mut ins = Insights {
    kind: "account".into(),
    subject: page.clone(),
    title: info.str("name"),
    url: info.str("link"),
    from: Some(day(since)),
    to: Some(day(until)),
    ..Insights::default()
  };
  ins.total("followers", info.at("followers_count").clone());
  ins.total("page_likes", info.at("fan_count").clone());
  for metric in PAGE_METRICS {
    let q = vec![
      (s("metric"), (*metric).to_owned()),
      (s("period"), s("day")),
      (s("since"), since.as_second().to_string()),
      (s("until"), until.as_second().to_string()),
    ];
    match g.get(&format!("{page}/insights"), &q).await {
      Ok(v) => add_series(&mut ins, metric, &v),
      Err(e) => ins.warnings.push(format!("{metric}: {}", e.message)),
    }
  }
  Ok(ins)
}

/// A daily metric as a series, summed into `totals` (follows as the last value).
fn add_series(ins: &mut Insights, metric: &str, v: &Value) {
  let Some(entry) = v.list("data").first() else {
    return;
  };
  let mut points = Vec::new();
  let mut sum = 0.0;
  for p in entry.list("values") {
    let value = p.at("value");
    if let Some(n) = value.as_f64() {
      sum += n;
    }
    if let Some(t) = p.str("end_time").and_then(|t| graph_time(&t)) {
      points.push(Point {
        date: day(t),
        value: value.clone(),
      });
    }
  }
  let name = metric.trim_start_matches("page_").to_owned();
  if !points.is_empty() {
    let last = points.last().map(|p| p.value.clone());
    let total = if metric == "page_follows" {
      last.unwrap_or_default()
    } else {
      json!(sum)
    };
    ins.total(&name, total);
    ins.series.push(Series {
      metric: name,
      points,
    });
  }
}

async fn post_insights(c: &Client, arg: &str) -> Result<Insights> {
  let id = graph_id(arg)?;
  let g = match owner_of(&id) {
    Some(owner) => as_page(c, owner).await?,
    None => c.graph()?,
  };
  let p = post(&g.get(&id, &[(s("fields"), s(POST_FIELDS))]).await?);
  let mut ins = Insights {
    kind: "post".into(),
    subject: id.clone(),
    title: p.text.as_deref().map(|t| t.chars().take(60).collect()),
    url: p.url.clone(),
    totals_period: Some("lifetime".into()),
    ..Insights::default()
  };
  ins.total("likes", p.metrics.likes.map(Value::from));
  ins.total("comments", p.metrics.comments.map(Value::from));
  ins.total("shares", p.metrics.shares.map(Value::from));
  for metric in POST_METRICS {
    let q = vec![(s("metric"), (*metric).to_owned())];
    match g.get(&format!("{id}/insights"), &q).await {
      Ok(v) => {
        let Some(entry) = v.list("data").first() else {
          continue;
        };
        let value = entry.at("values.0.value").clone();
        let name = metric.trim_start_matches("post_").to_owned();
        if let Value::Object(by) = &value {
          ins.breakdowns.push(Breakdown {
            dimension: name,
            period: Some("lifetime".into()),
            items: by
              .iter()
              .map(|(k, v)| Share {
                label: k.to_lowercase(),
                value: v.clone(),
                ..Share::default()
              })
              .collect(),
          });
        } else {
          ins.total(&name, value);
        }
      }
      Err(e) => ins.warnings.push(format!("{metric}: {}", e.message)),
    }
  }
  Ok(ins)
}

fn day(t: Timestamp) -> String {
  t.strftime("%Y-%m-%d").to_string()
}

// ── Messenger (Page inbox) ─────────────────────────────────────────────────

pub async fn conversations(
  c: &Client,
  page: Option<&str>,
  req: &PageReq,
) -> Result<Page<Collection>> {
  let page = page_id(c, page).await?;
  let g = as_page(c, &page).await?;
  let (items, next) = g
    .page(
      &format!("{page}/conversations"),
      &[(
        s("fields"),
        s("id,updated_time,message_count,unread_count,snippet,participants,link"),
      )],
      req,
      100,
    )
    .await?;
  let chats = items
    .iter()
    .map(|v| {
      let other = v
        .list("participants.data")
        .iter()
        .find(|p| p.str("id").as_deref() != Some(page.as_str()))
        .cloned()
        .unwrap_or_default();
      Collection {
        id: v.str("id").unwrap_or_default(),
        kind: "dm".into(),
        name: other.str("name").unwrap_or_default(),
        description: v.str("snippet"),
        url: v
          .str("link")
          .map(|l| format!("https://www.facebook.com{l}")),
        items: v.count("message_count"),
        extra: [
          ("page".to_owned(), Value::from(page.clone())),
          ("updated_time".to_owned(), v.at("updated_time").clone()),
          ("unread".to_owned(), v.at("unread_count").clone()),
          ("participant".to_owned(), other.at("id").clone()),
        ]
        .into_iter()
        .filter(|(_, v)| !v.is_null())
        .collect(),
        raw: Some(v.clone()),
        ..Collection::default()
      }
    })
    .collect();
  Ok(Page::new(chats, next))
}

pub async fn messages(c: &Client, conv: &str, req: &PageReq) -> Result<Page<Post>> {
  let page = page_id(c, None).await.ok();
  let g = match &page {
    Some(p) => as_page(c, p).await?,
    None => c.graph()?,
  };
  let (items, next) = g
    .page(
      &format!("{conv}/messages"),
      &[(
        s("fields"),
        s("id,message,from,to,created_time,attachments,sticker"),
      )],
      req,
      100,
    )
    .await?;
  let posts = items
    .iter()
    .map(|m| {
      let mut media = Vec::new();
      for a in m.list("attachments.data") {
        if let Some(u) = a.str("image_data.url") {
          media.push(Media::image(u));
        } else if let Some(u) = a.str("video_data.url") {
          media.push(Media::video(u));
        } else if let Some(u) = a.str("file_url") {
          media.push(Media::new(social_core::MediaKind::File, u));
        }
      }
      Post {
        id: format!("{conv}/{}", m.str("id").unwrap_or_default()),
        kind: "message".into(),
        text: m.str("message"),
        author: m.at("from").is_object().then(|| User {
          id: m.str("from.id").unwrap_or_default(),
          name: m.str("from.name").unwrap_or_default(),
          handle: m.str("from.email"),
          ..User::default()
        }),
        created_at: m.str("created_time").and_then(|t| graph_time(&t)),
        media,
        extra: [("chat".to_owned(), Value::from(conv))].into(),
        raw: Some(m.clone()),
        ..Post::default()
      }
    })
    .collect();
  Ok(Page::new(posts, next))
}

/// Answer in a Page conversation (a conversation id `t_…` or a PSID).
pub async fn send(c: &Client, chat: &str, msg: &Message) -> Result<Action> {
  if !msg.files.is_empty() {
    return Err(Error::unsupported(
      "send --attach on Facebook Pages (send a link instead)",
    ));
  }
  let page = page_id(c, None).await?;
  let g = as_page(c, &page).await?;
  let psid =
    if chat.starts_with("t_") || chat.contains('_') && !chat.bytes().all(|b| b.is_ascii_digit()) {
      let v = g.get(chat, &[(s("fields"), s("participants"))]).await?;
      v.list("participants.data")
        .iter()
        .find_map(|p| p.str("id").filter(|id| *id != page))
        .ok_or_else(|| Error::not_found("no other participant in this conversation"))?
    } else {
      chat.to_owned()
    };
  let mut form: Params = vec![
    (s("recipient"), json!({"id": psid}).to_string()),
    (s("message"), json!({"text": msg.text}).to_string()),
    (s("messaging_type"), s("RESPONSE")),
  ];
  if let Some(r) = &msg.reply_to {
    form.push((
      s("reply_to"),
      json!({"mid": r.rsplit('/').next().unwrap_or(r)}).to_string(),
    ));
  }
  let v = g.post(&format!("{page}/messages"), &form).await?;
  Ok(Action::done("send", chat).with_id(v.str("message_id").unwrap_or_default()))
}

/// The person or Page a Graph token belongs to.
pub async fn me(c: &Client) -> Result<User> {
  let g = c.graph()?;
  let v = g.get("me", &[(s("fields"), s("id,name,link"))]).await?;
  Ok(User {
    id: v.str("id").unwrap_or_default(),
    name: v.str("name").unwrap_or_default(),
    url: v.str("link"),
    extra: [("source".to_owned(), Value::from("graph-api"))].into(),
    raw: Some(v),
    ..User::default()
  })
}

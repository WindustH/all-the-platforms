//! Published articles: your own (with read / like / 在看 / share / comment
//! counts from the backend, or the API's freepublish list), and other
//! accounts' (backend, by their `fakeid`, which is their `__biz`).
//!
//! The backend's `appmsgpublish` nests JSON in strings twice: `publish_page`
//! holds `publish_list[]`, whose `publish_info` holds the articles
//! (`appmsg_info` with stats for your own account, `appmsgex` otherwise).

use atp_core::text::from_secs;
use atp_core::{Page, PageReq, Post, Result, Value, ValueExt, json};

use crate::accounts::Account;
use crate::api::Api;
use crate::backend::nested;
use crate::refs;

/// One page of an account's published articles.
pub async fn page(api: &Api, account: &Account, req: &PageReq) -> Result<Page<Post>> {
  match account {
    Account::Own if !api.backend().has() => api_page(api, req).await,
    Account::Own => web_page(api, None, req).await,
    Account::Other { fakeid, .. } => {
      api.backend().require()?;
      web_page(api, Some(fakeid), req).await
    }
  }
}

async fn web_page(api: &Api, fakeid: Option<&str>, req: &PageReq) -> Result<Page<Post>> {
  let begin = req.number_or(0);
  let count = req.size_within(20);
  let mut query = vec![
    ("sub", "list".to_owned()),
    ("begin", begin.to_string()),
    ("count", count.to_string()),
  ];
  // Your own list without `sub_action` carries the counts; others' needs it.
  if let Some(f) = fakeid {
    query.extend([
      ("search_field", "null".to_owned()),
      ("query", String::new()),
      ("fakeid", f.to_owned()),
      ("type", "101_1".into()),
      ("free_publish_type", "1".into()),
      ("sub_action", "list_ex".into()),
    ]);
  }
  let v = api.backend().get("/cgi-bin/appmsgpublish", query).await?;
  let page = nested(&v, "publish_page");
  let bizuin = match fakeid {
    Some(f) => refs::bizuin(f),
    None => api
      .ctx
      .http
      .cookie("bizuin")
      .or_else(|| api.ctx.http.cookie("data_bizuin")),
  };
  let groups = page.list("publish_list");
  let mut posts = Vec::new();
  for g in groups {
    let info = nested(g, "publish_info");
    let sent = info.i64("sent_info.time");
    for a in info.list("appmsg_info") {
      posts.push(own_item(a, bizuin.as_deref(), sent));
    }
    if info.list("appmsg_info").is_empty() {
      for a in info.list("appmsgex") {
        posts.push(ex_item(a, bizuin.as_deref()));
      }
    }
  }
  let total = page.u64("total_count").unwrap_or(0);
  let next = (groups.len() >= count && (total == 0 || begin + (count as u64) < total))
    .then(|| (begin + count as u64).to_string());
  Ok(Page::new(posts, next))
}

fn post_id(bizuin: Option<&str>, appmsgid: &str, itemidx: &str) -> String {
  match bizuin {
    Some(b) => format!("{b}_{appmsgid}_{itemidx}"),
    None => format!("{appmsgid}_{itemidx}"),
  }
}

/// `appmsg_info` of your own list: the article with its counts.
fn own_item(a: &Value, bizuin: Option<&str>, sent: Option<i64>) -> Post {
  let mid = a.str("appmsgid").unwrap_or_default();
  let idx = a.str("itemidx").unwrap_or_else(|| "1".into());
  let mut post = Post {
    id: post_id(bizuin, &mid, &idx),
    kind: kind(a.str("item_show_type").as_deref()).into(),
    title: a.str("title"),
    text: a.str("digest"),
    url: a
      .first_str(&["content_url", "link"])
      .and_then(|u| refs::article_url(&u).ok()),
    created_at: a
      .i64("line_info.send_time")
      .or(sent)
      .or_else(|| a.i64("create_time"))
      .and_then(from_secs),
    raw: Some(a.clone()),
    ..Post::default()
  };
  let m = &mut post.metrics;
  m.views = a.u64("read_num");
  m.likes = a.u64("like_num");
  m.comments = a.u64("comment_num");
  m.shares = a.u64("share_num");
  for (key, path) in [
    ("zaikan", "old_like_num"),
    ("moment_likes", "moment_like_num"),
    ("reprints", "reprint_num"),
    (
      "comments_with_replies",
      "total_comment_count_contains_reply",
    ),
  ] {
    if let Some(n) = a.u64(path) {
      m.other.insert(key.into(), n);
    }
  }
  extras(&mut post, a, &mid, &idx);
  post
}

/// `appmsgex` of a list: the article without counts.
fn ex_item(a: &Value, bizuin: Option<&str>) -> Post {
  let mid = a.str("appmsgid").unwrap_or_default();
  let idx = a.str("itemidx").unwrap_or_else(|| "1".into());
  let mut post = Post {
    id: post_id(bizuin, &mid, &idx),
    kind: kind(a.str("item_show_type").as_deref()).into(),
    title: a.str("title"),
    text: a.str("digest"),
    url: a.str("link").and_then(|u| refs::article_url(&u).ok()),
    created_at: a.i64("create_time").and_then(from_secs),
    raw: Some(a.clone()),
    ..Post::default()
  };
  if let Some(name) = a.str("author_name") {
    post.extra.insert("writer".into(), name.into());
  }
  extras(&mut post, a, &mid, &idx);
  post
}

fn extras(post: &mut Post, a: &Value, mid: &str, idx: &str) {
  post.extra.insert("mid".into(), mid.into());
  post.extra.insert("idx".into(), idx.into());
  for (key, path) in [
    ("cover", "cover"),
    ("comment_id", "comment_id"),
    ("album_id", "album_id"),
  ] {
    if let Some(v) = a.str(path).filter(|v| v != "0") {
      post.extra.insert(key.into(), v.into());
    }
  }
  if a.i64("is_deleted") == Some(1) {
    post.extra.insert("deleted".into(), true.into());
  }
  if let Some(copyright) = a.i64("copyright_stat").or_else(|| a.i64("copyright_type")) {
    post
      .extra
      .insert("original".into(), (copyright == 1).into());
  }
}

/// One of your articles in your 50 newest published ones (backend).
pub async fn find_own(api: &Api, mid: &str, idx: u64) -> Option<Post> {
  for page in 0..5u64 {
    let req = PageReq {
      cursor: Some((page * 10).to_string()),
      size: 10,
    };
    let posts = web_page(api, None, &req).await.ok()?;
    let hit = posts.items.into_iter().find(|p| {
      let get = |k: &str| p.extra.get(k).and_then(Value::as_str).map(str::to_owned);
      get("mid").as_deref() == Some(mid) && get("idx").and_then(|i| i.parse().ok()) == Some(idx)
    });
    if hit.is_some() || posts.next_cursor.is_none() {
      return hit;
    }
  }
  None
}

/// Counts of an article read from its page, when it is one of yours.
pub async fn add_own_counts(api: &Api, post: &mut Post) {
  let get = |k: &str| post.extra.get(k).and_then(Value::as_str).map(str::to_owned);
  let own = api
    .ctx
    .http
    .cookie("bizuin")
    .or_else(|| api.ctx.http.cookie("data_bizuin"));
  let (Some(biz), Some(mid), Some(idx)) = (get("biz"), get("mid"), get("idx")) else {
    return;
  };
  if own.is_none() || refs::bizuin(&biz) != own {
    return;
  }
  if let Some(mine) = find_own(api, &mid, idx.parse().unwrap_or(1)).await {
    post.metrics = mine.metrics;
  }
}

pub fn kind(show_type: Option<&str>) -> &'static str {
  match show_type {
    Some("5") => "video",
    Some("6") => "music",
    Some("7") => "audio",
    Some("8") => "images",
    Some("10") => "text",
    Some("11") => "share",
    Some("17") => "short",
    _ => "article",
  }
}

/// Your published articles through the API (`freepublish/batchget`):
/// verified accounts only, and only posts published without notification.
async fn api_page(api: &Api, req: &PageReq) -> Result<Page<Post>> {
  let offset = req.number_or(0);
  let count = req.size_within(20);
  let v = api
    .official()
    .post(
      "/cgi-bin/freepublish/batchget",
      &json!({ "offset": offset, "count": count, "no_content": 1 }),
    )
    .await?;
  let mut posts = Vec::new();
  for item in v.list("item") {
    let article_id = item.str("article_id").unwrap_or_default();
    for (i, n) in item.list("content.news_item").iter().enumerate() {
      let mut post = Post {
        id: format!("{article_id}/{}", i + 1),
        kind: "article".into(),
        title: n.str("title"),
        text: n.str("digest"),
        url: n.str("url").and_then(|u| refs::article_url(&u).ok()),
        created_at: item.i64("update_time").and_then(from_secs),
        raw: Some(n.clone()),
        ..Post::default()
      };
      post
        .extra
        .insert("article_id".into(), article_id.clone().into());
      post.extra.insert("index".into(), (i + 1).into());
      if let Some(a) = n.str("author") {
        post.extra.insert("writer".into(), a.into());
      }
      if n.i64("is_deleted") == Some(1) {
        post.extra.insert("deleted".into(), true.into());
      }
      posts.push(post);
    }
  }
  let total = v.u64("total_count").unwrap_or(0);
  let next = (offset + (count as u64) < total).then(|| (offset + count as u64).to_string());
  Ok(Page::new(posts, next))
}

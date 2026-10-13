//! Search (`search.messages`, user tokens only): Slack's own query syntax
//! (`in:#channel`, `from:@user`, `has:link`, `before:` / `after:` …), and
//! mentions of the account as notifications.

use social_core::{Notification, Page, PageReq, Post, Query, Result, Value, ValueExt};

use crate::api::Api;
use crate::chats::ChatInfo;
use crate::messages;
use crate::parse;
use crate::users;

pub const SORTS: &[&str] = &["relevance", "newest"];

async fn matches(
  api: &Api,
  query: &str,
  sort: &str,
  page: &PageReq,
) -> Result<(Vec<Post>, Option<String>)> {
  api.require_user("search")?;
  let no = page.number_or(1);
  let v = api
    .call(
      "search.messages",
      vec![
        ("query", query.to_owned()),
        ("count", page.size_within(100).to_string()),
        ("page", no.to_string()),
        ("sort", sort.to_owned()),
        ("sort_dir", "desc".to_owned()),
      ],
    )
    .await?;
  let found = v.at("messages");
  let rows: Vec<Value> = found.list("matches").to_vec();
  users::resolve(api, parse::user_ids(&rows)).await;
  for m in &rows {
    if let (Some(id), Some(name)) = (m.str("channel.id"), m.str("channel.name")) {
      let kind = if m.bool("channel.is_im") == Some(true) {
        "dm"
      } else if m.bool("channel.is_private") == Some(true) {
        "private"
      } else {
        "channel"
      };
      let name = if kind == "dm" {
        name
      } else {
        format!("#{name}")
      };
      api.chats.borrow_mut().entry(id).or_insert(ChatInfo {
        name,
        kind: kind.into(),
      });
    }
  }
  let posts = messages::render(api, |view| {
    rows
      .iter()
      .map(|m| {
        let channel = m.str("channel.id").unwrap_or_default();
        let mut p = parse::message(m, &channel, view);
        if let Some(link) = m.str("permalink") {
          p.url = Some(link);
        }
        p
      })
      .collect()
  })
  .await?;
  let pages = found.u64("paging.pages").unwrap_or(1);
  let next = (no < pages).then(|| (no + 1).to_string());
  Ok((posts, next))
}

pub async fn messages(api: &Api, q: &Query, page: &PageReq) -> Result<Page<Post>> {
  let sort = match q.sort.as_deref() {
    Some("newest") => "timestamp",
    _ => "score",
  };
  let (items, next) = matches(api, &q.keyword, sort, page).await?;
  Ok(Page::new(items, next))
}

/// Messages that mention the account, newest first.
pub async fn mentions(api: &Api, page: &PageReq) -> Result<Page<Notification>> {
  api.require_user("notifications")?;
  let me = api.me().await?;
  let (posts, next) = matches(api, &format!("<@{me}>"), "timestamp", page).await?;
  let items = posts
    .into_iter()
    .map(|p| Notification {
      id: p.id.clone(),
      kind: "mention".into(),
      text: p.text.clone().unwrap_or_default(),
      actor: p.author.clone(),
      target: p
        .extra
        .get("chat_name")
        .and_then(|v| v.as_str())
        .map(str::to_owned),
      url: p.url.clone(),
      created_at: p.created_at,
      unread: None,
      raw: p.raw,
    })
    .collect();
  Ok(Page::new(items, next))
}

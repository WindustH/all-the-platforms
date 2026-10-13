//! The mentions inbox of a user account.

use social_core::{Notification, Page, PageReq, Result, ValueExt};

use crate::api::Api;
use crate::parse;

/// `all` (mentions, role and @everyone pings) or `direct` (only you by name).
pub const KINDS: &[&str] = &["all", "direct"];

pub async fn mentions(api: &Api, kind: Option<&str>, page: &PageReq) -> Result<Page<Notification>> {
  api.require_user("notifications")?;
  let wide = kind != Some("direct");
  let size = page.size_within(100);
  let mut query = vec![
    ("limit", size.to_string()),
    ("roles", wide.to_string()),
    ("everyone", wide.to_string()),
  ];
  if let Some(before) = &page.cursor {
    query.push(("before", before.clone()));
  }
  let v = api.get("/users/@me/mentions", &query).await?;
  let rows = v.as_array().cloned().unwrap_or_default();
  let next = (rows.len() >= size)
    .then(|| rows.last().and_then(|m| m.str("id")))
    .flatten();
  let items = rows
    .iter()
    .map(|m| {
      let post = parse::message(m, None, None);
      Notification {
        id: post.id.clone(),
        kind: "mention".into(),
        text: post.text.clone().unwrap_or_default(),
        actor: post.author.clone(),
        target: m.str("channel_id"),
        url: post.url.clone(),
        created_at: post.created_at,
        unread: None,
        raw: post.raw,
      }
    })
    .collect();
  Ok(Page::new(items, next))
}

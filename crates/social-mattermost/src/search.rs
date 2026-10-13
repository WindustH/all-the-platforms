//! Post search across your teams (`team:NAME` in the query narrows it to
//! one; Mattermost's own `from:`, `in:`, `on:`, `before:`, `after:` work),
//! mentions as notifications, and unread counts per team.

use std::collections::BTreeMap;

use social_core::{Notification, Page, PageReq, Post, Query, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::{chats, parse, posts};

async fn find(api: &Api, keyword: &str, page: &PageReq) -> Result<(Vec<Post>, Option<String>)> {
  let mut team = None;
  let terms: Vec<&str> = keyword
    .split_whitespace()
    .filter(|w| match w.strip_prefix("team:") {
      Some(t) => {
        team = Some(t.to_owned());
        false
      }
      None => true,
    })
    .collect();
  let terms = terms.join(" ");
  let teams: Vec<String> = match team {
    Some(t) => vec![chats::team_id(api, &t).await?],
    None => {
      let v = api.get("/users/me/teams", &[]).await?;
      v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|t| t.str("id"))
        .collect()
    }
  };
  let per_page = page.size_within(100);
  let no = page.number_or(0);
  let mut rows: Vec<Value> = Vec::new();
  let mut more = false;
  for t in teams {
    let v = api
      .post_read(
        &format!("/teams/{t}/posts/search"),
        json!({
          "terms": terms,
          "is_or_search": false,
          "time_zone_offset": 0,
          "include_deleted_channels": false,
          "page": no,
          "per_page": per_page,
        }),
      )
      .await?;
    let found = parse::ordered(&v);
    more |= found.len() >= per_page;
    rows.extend(found);
  }
  rows.sort_by_key(|p| std::cmp::Reverse(p.i64("create_at").unwrap_or_default()));
  let next = more.then(|| (no + 1).to_string());
  Ok((posts::convert(api, &rows).await?, next))
}

pub async fn messages(api: &Api, q: &Query, page: &PageReq) -> Result<Page<Post>> {
  let (items, next) = find(api, &q.keyword, page).await?;
  Ok(Page::new(items, next))
}

/// Posts that mention you (`@username`), newest first.
pub async fn mentions(api: &Api, page: &PageReq) -> Result<Page<Notification>> {
  let me = api.get("/users/me", &[]).await?;
  let name = me.str("username").unwrap_or_default();
  let (found, next) = find(api, &format!("\"@{name}\""), page).await?;
  let items = found
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

/// Unread messages and mentions per team.
pub async fn unread(api: &Api) -> Result<BTreeMap<String, u64>> {
  chats::teams_known(api).await;
  let v = api.get("/users/me/teams/unread", &[]).await?;
  let teams = api.teams.borrow();
  let mut out = BTreeMap::new();
  for t in v.as_array().into_iter().flatten() {
    let id = t.str("team_id").unwrap_or_default();
    let name = teams.get(&id).cloned().unwrap_or(id);
    out.insert(
      format!("{name}.messages"),
      t.u64("msg_count").unwrap_or_default(),
    );
    out.insert(
      format!("{name}.mentions"),
      t.u64("mention_count").unwrap_or_default(),
    );
  }
  Ok(out)
}

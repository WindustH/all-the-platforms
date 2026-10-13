//! The account, profiles and the notification list.

use social_core::text::from_millis;
use social_core::{Notification, Page, PageReq, Result, User, ValueExt};

use crate::api::{Api, seg};
use crate::parse::{self, Members};
use crate::refs;

pub async fn my_id(api: &Api) -> Result<String> {
  if let Some(id) = api.user_id() {
    return Ok(id);
  }
  let v = api.get("/account/whoami", &[]).await?;
  let id = v.str("user_id").unwrap_or_default();
  api.ctx.set_extra("user_id", &id);
  Ok(id)
}

pub async fn profile(api: &Api, id: &str) -> Result<User> {
  let v = api
    .call(
      social_core::http::Method::GET,
      &format!("/profile/{}", seg(id)),
      &[],
      None,
    )
    .await?;
  let mut u = parse::user(id, v.str("displayname"), v.str("avatar_url"));
  if let Some(mxc) = v.str("avatar_url") {
    u.avatar = api.media_url(&mxc).await;
  }
  Ok(u)
}

pub async fn whoami(api: &Api) -> Result<User> {
  let v = api.get("/account/whoami", &[]).await?;
  let id = v.str("user_id").unwrap_or_default();
  api.ctx.set_extra("user_id", &id);
  let mut u = profile(api, &id).await?;
  if let Some(d) = v.str("device_id") {
    u.extra.insert("device_id".into(), d.into());
  }
  Ok(u)
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let id = match refs::user(arg)? {
    Some(id) => id,
    None => my_id(api).await?,
  };
  profile(api, &id).await
}

/// `/notifications`: messages that notified you; `--type highlight` keeps
/// mentions and keywords.
pub async fn notifications(
  api: &Api,
  kind: Option<&str>,
  page: &PageReq,
) -> Result<Page<Notification>> {
  let mut query = vec![("limit", page.size_within(100).to_string())];
  if let Some(c) = &page.cursor {
    query.push(("from", c.clone()));
  }
  if kind == Some("highlight") {
    query.push(("only", "highlight".into()));
  }
  let v = api.get("/notifications", &query).await?;
  let members = Members::default();
  let items = v
    .list("notifications")
    .iter()
    .map(|n| {
      let e = n.at("event");
      let rid = n.str("room_id").unwrap_or_default();
      let post = parse::event(&rid, "", e, &members);
      let highlight = n
        .list("actions")
        .iter()
        .any(|a| a.str("set_tweak").as_deref() == Some("highlight"));
      Notification {
        id: format!("{rid}/{}", e.str("event_id").unwrap_or_default()),
        kind: if highlight { "mention" } else { "message" }.into(),
        text: post
          .as_ref()
          .and_then(|p| p.text.clone())
          .unwrap_or_default(),
        actor: e.str("sender").map(|s| members.user(&s)),
        target: Some(rid.clone()),
        url: post.and_then(|p| p.url),
        created_at: n.i64("ts").and_then(from_millis),
        unread: n.bool("read").map(|r| !r),
        raw: Some(n.clone()),
      }
    })
    .collect();
  Ok(Page::new(items, v.str("next_token")))
}

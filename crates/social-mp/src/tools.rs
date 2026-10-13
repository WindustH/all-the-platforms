//! Read-only odds and ends: the menu and auto-replies as configured (every
//! account type, API), API quotas and request lookups, and the accounts a
//! backend session may switch between.

use social_core::{Page, Result, User, Value, ValueExt, json};

use crate::client::Client;

pub async fn menu(c: &Client) -> Result<Value> {
  c.api_get("/cgi-bin/get_current_selfmenu_info", &[]).await
}

pub async fn autoreply(c: &Client) -> Result<Value> {
  c.api_get("/cgi-bin/get_current_autoreply_info", &[]).await
}

/// Daily quota of one API path (`/cgi-bin/draft/add`).
pub async fn quota(c: &Client, path: &str) -> Result<Value> {
  let path = if path.starts_with('/') {
    path.to_owned()
  } else {
    format!("/{path}")
  };
  let v = c
    .api_post("/cgi-bin/openapi/quota/get", &json!({ "cgi_path": path }))
    .await?;
  Ok(json!({ "path": path, "quota": v.at("quota"), "rate_limit": v.at("rate_limit") }))
}

/// What happened to a request whose error carried an `rid`.
pub async fn rid(c: &Client, rid: &str) -> Result<Value> {
  let v = c
    .api_post("/cgi-bin/openapi/rid/get", &json!({ "rid": rid }))
    .await?;
  Ok(v.at("request").clone())
}

/// Accounts the backend session can switch to.
pub async fn accounts(c: &Client) -> Result<Page<User>> {
  c.require_web()?;
  let v = c
    .web_get(
      "/cgi-bin/switchacct",
      vec![
        ("action", "get_acct_list".into()),
        ("fingerprint", c.fingerprint()),
      ],
    )
    .await?;
  let mut users = Vec::new();
  for (kind, key) in [
    ("subscription", "biz_list.list"),
    ("service", "service_biz_list.list"),
    ("sub", "sub_biz_list.list"),
  ] {
    for a in v.list(key) {
      let mut u = User {
        id: a.str("username").unwrap_or_default(),
        name: a.str("nickname").unwrap_or_default(),
        avatar: a.str("headimgurl"),
        raw: Some(a.clone()),
        ..User::default()
      };
      u.extra.insert("type".into(), kind.into());
      users.push(u);
    }
  }
  Ok(Page::last(users))
}

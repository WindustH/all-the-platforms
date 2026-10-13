//! Accounts: the one acted as, profiles, the accounts a server offers, and
//! friends.

use atp_core::{Page, Result, User, Value, ValueExt, json};

use crate::api::Api;
use crate::parse;

pub async fn whoami(api: &Api) -> Result<User> {
  let v = api.call("login.get", json!({})).await?;
  let mut user = parse::user(v.at("user"));
  for key in ["platform", "status", "adapter"] {
    if let Some(value) = v.str(key) {
      user.extra.insert(key.into(), json!(value));
    }
  }
  if let Some(f) = v.as_object().and_then(|o| o.get("features")) {
    user.extra.insert("features".into(), f.clone());
  }
  Ok(user)
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let id = arg.trim().trim_start_matches("user:");
  Ok(parse::user(
    &api.call("user.get", json!({"user_id": id})).await?,
  ))
}

/// The accounts the server offers, with what `--token` picks them by.
pub async fn logins(api: &Api) -> Result<Value> {
  let server = api.require()?;
  let logins = api.logins(&server).await?;
  Ok(Value::from(
    logins
      .iter()
      .map(|l| {
        let platform = l.str("platform").unwrap_or_default();
        let user_id = l.str("user.id").unwrap_or_default();
        json!({
          "platform": l.str("platform"),
          "user_id": l.str("user.id"),
          "name": l.str("user.name"),
          "status": l.at("status"),
          "pick": format!("{platform}:{user_id}"),
        })
      })
      .collect::<Vec<_>>(),
  ))
}

/// Friends of the account, from an earlier `next` when given.
pub async fn friends(api: &Api, cursor: Option<String>) -> Result<Value> {
  let mut body = json!({});
  if let Some(c) = cursor {
    body["next"] = json!(c);
  }
  api.call("friend.list", body).await
}

/// `{data, next}` of a list method.
pub fn page(v: &Value) -> (Vec<Value>, Option<String>) {
  (v.list("data").to_vec(), v.str("next"))
}

pub async fn members(api: &Api, guild: &str, cursor: Option<&str>) -> Result<Page<User>> {
  let mut body = json!({"guild_id": guild});
  if let Some(n) = cursor {
    body["next"] = json!(n);
  }
  let (rows, next) = page(&api.call("guild.member.list", body).await?);
  Ok(Page::new(rows.iter().map(parse::member).collect(), next))
}

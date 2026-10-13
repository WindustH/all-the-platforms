//! Users, guild members and profiles.

use atp_core::{User, Value, ValueExt, json};

use super::{CDN, snowflake_time};
use crate::api::WEB;

fn avatar(v: &Value) -> Option<String> {
  let id = v.str("id")?;
  Some(match v.str("avatar") {
    Some(hash) => {
      let ext = if hash.starts_with("a_") { "gif" } else { "png" };
      format!("{CDN}/avatars/{id}/{hash}.{ext}?size=256")
    }
    None => {
      // Default avatars: by id for new usernames, by discriminator for old tags.
      let index = match v.str("discriminator").and_then(|d| d.parse::<u64>().ok()) {
        Some(d) if d > 0 => d % 5,
        _ => (id.parse::<u64>().unwrap_or_default() >> 22) % 6,
      };
      format!("{CDN}/embed/avatars/{index}.png")
    }
  })
}

/// `name` (`#1234` for legacy tags).
fn handle(v: &Value) -> Option<String> {
  let name = v.str("username")?;
  Some(match v.str("discriminator").filter(|d| d != "0") {
    Some(d) => format!("{name}#{d}"),
    None => name,
  })
}

pub fn user(v: &Value) -> User {
  let id = v.str("id").unwrap_or_default();
  let mut u = User {
    name: v
      .first_str(&["global_name", "username"])
      .unwrap_or_default(),
    handle: handle(v),
    url: (!id.is_empty()).then(|| format!("{WEB}/users/{id}")),
    avatar: avatar(v),
    bio: v.str("bio"),
    created_at: snowflake_time(&id),
    id,
    raw: Some(v.clone()),
    ..User::default()
  };
  for key in ["bot", "system"] {
    if v.bool(key) == Some(true) {
      u.extra.insert(key.into(), json!(true));
    }
  }
  if let Some(p) = v.str("pronouns") {
    u.extra.insert("pronouns".into(), json!(p));
  }
  u
}

/// A guild member: the user with the guild's nickname and roles.
pub fn member(v: &Value) -> User {
  let mut u = user(v.at("user"));
  if let Some(nick) = v.str("nick") {
    u.extra.insert("global_name".into(), json!(u.name));
    u.name = nick;
  }
  for key in ["joined_at", "premium_since"] {
    if let Some(t) = v.str(key) {
      u.extra.insert(key.into(), json!(t));
    }
  }
  let roles = v.list("roles");
  if !roles.is_empty() {
    u.extra.insert("roles".into(), json!(roles));
  }
  u.raw = Some(v.clone());
  u
}

/// `/users/{id}/profile`: the user, their bio, connections and mutual servers.
pub fn profile(v: &Value) -> User {
  let mut u = user(v.at("user"));
  u.bio = v
    .first_str(&["user_profile.bio", "user.bio"])
    .filter(|b| !b.is_empty());
  if let Some(p) = v.str("user_profile.pronouns").filter(|p| !p.is_empty()) {
    u.extra.insert("pronouns".into(), json!(p));
  }
  let connections: Vec<Value> = v
    .list("connected_accounts")
    .iter()
    .map(|c| json!({"type": c.str("type"), "name": c.str("name"), "verified": c.bool("verified")}))
    .collect();
  if !connections.is_empty() {
    u.extra.insert("connections".into(), json!(connections));
  }
  let mutual = v.list("mutual_guilds");
  if !mutual.is_empty() {
    u.extra.insert("mutual_servers".into(), json!(mutual.len()));
  }
  if let Some(n) = v.u64("mutual_friends_count") {
    u.extra.insert("mutual_friends".into(), json!(n));
  }
  if let Some(t) = v.str("premium_since") {
    u.extra.insert("nitro_since".into(), json!(t));
  }
  u.raw = Some(v.clone());
  u
}

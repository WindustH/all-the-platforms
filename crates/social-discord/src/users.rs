//! Accounts: the logged-in one, profiles, and relationships (friends,
//! requests, blocks) of a user account.

use social_core::{Error, ErrorCode, Page, Result, User, Value, ValueExt, json};

use crate::api::{Api, Kind, Token};
use crate::{parse, refs};

pub async fn whoami(api: &Api) -> Result<User> {
  let token = api.require()?;
  let v = api.get("/users/@me", &[]).await?;
  Ok(me(&v, token.kind))
}

fn me(v: &Value, kind: Kind) -> User {
  let mut u = parse::user(v);
  u.extra.insert("token".into(), json!(kind.name()));
  for key in ["mfa_enabled", "verified"] {
    if let Some(b) = v.bool(key) {
      u.extra.insert(key.into(), json!(b));
    }
  }
  if let Some(t) = v.u64("premium_type").filter(|t| *t > 0) {
    u.extra.insert("nitro".into(), json!(t));
  }
  // The account's email and phone stay in `raw`.
  u
}

/// Check a token: as given, or (without a `Bot` prefix) as a user token,
/// then as a bot token. Returns the kind Discord accepted it as.
pub async fn verify(api: &Api, raw: &str) -> Result<Token> {
  let (secret, kind) = Token::parse(raw);
  if secret.is_empty() {
    return Err(Error::input("the token is empty"));
  }
  let kinds = match kind {
    Some(k) => vec![k],
    None => vec![Kind::User, Kind::Bot],
  };
  let mut last = None;
  for kind in kinds {
    let token = Token {
      secret: secret.clone(),
      kind,
    };
    match api
      .send(
        Some(&token),
        social_core::http::Method::GET,
        "/users/@me",
        &[],
        crate::api::Body::None,
      )
      .await
    {
      Ok(v) if v.str("id").is_some() => {
        // A bot token sent as a user token is refused; the reverse too.
        let is_bot = v.bool("bot") == Some(true);
        if is_bot == (kind == Kind::Bot) {
          return Ok(token);
        }
      }
      Ok(_) => {}
      Err(e) if e.code == ErrorCode::NotAuthenticated => last = Some(e),
      Err(e) => return Err(e),
    }
  }
  Err(last.unwrap_or_else(|| Error::auth("Discord did not accept the token")))
}

/// A profile: user accounts see bio, connections and mutual servers; bots the basics.
pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let arg = arg.trim();
  if matches!(arg, "me" | "@me") {
    return whoami(api).await;
  }
  let id = refs::user(arg)?;
  if !api.is_bot() {
    let profile = api
      .get(
        &format!("/users/{id}/profile"),
        &[
          ("with_mutual_guilds", "true".into()),
          ("with_mutual_friends_count", "true".into()),
        ],
      )
      .await;
    match profile {
      Ok(v) => return Ok(parse::profile(&v)),
      // Profiles of people without a shared server or friendship are closed.
      Err(e) if e.code == ErrorCode::PermissionDenied || e.code == ErrorCode::NotFound => {}
      Err(e) => return Err(e),
    }
  }
  Ok(parse::user(&api.get(&format!("/users/{id}"), &[]).await?))
}

const RELATIONS: [&str; 5] = ["", "friend", "blocked", "incoming", "outgoing"];

/// Friends and pending requests (`--all`: also blocked accounts).
pub async fn relationships(api: &Api, all: bool) -> Result<Page<User>> {
  api.require_user("friends")?;
  let v = api.get("/users/@me/relationships", &[]).await?;
  let items = v
    .as_array()
    .map(|rows| {
      rows
        .iter()
        .filter_map(|r| {
          let kind = RELATIONS
            .get(r.u64("type").unwrap_or_default() as usize)
            .copied()
            .unwrap_or("other");
          if kind == "blocked" && !all {
            return None;
          }
          let mut u = parse::user(r.at("user"));
          u.extra.insert("relation".into(), json!(kind));
          if let Some(nick) = r.str("nickname") {
            u.extra.insert("nickname".into(), json!(nick));
          }
          if let Some(since) = r.str("since") {
            u.extra.insert("since".into(), json!(since));
          }
          u.followed = Some(kind == "friend");
          Some(u)
        })
        .collect()
    })
    .unwrap_or_default();
  Ok(Page::last(items))
}

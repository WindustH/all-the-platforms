//! The app's bot, users, and display names for message authors.

use social_core::{Post, Result, User, Value, ValueExt};

use crate::api::Api;
use crate::{parse, refs};

/// The bot of the app (`/bot/v3/info`).
pub async fn whoami(api: &Api) -> Result<User> {
  let v = api.get("/bot/v3/info", &[]).await?;
  let bot = v.at("bot");
  let creds = api.require()?;
  Ok(User {
    id: bot.str("open_id").unwrap_or_default(),
    name: bot.str("app_name").unwrap_or_default(),
    avatar: bot.str("avatar_url"),
    extra: [
      ("app_id".to_owned(), Value::from(creds.app_id)),
      ("host".to_owned(), Value::from(creds.base)),
      (
        "activated".to_owned(),
        Value::from(bot.i64("activate_status") == Some(2)),
      ),
    ]
    .into_iter()
    .collect(),
    ..User::default()
  })
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let (kind, id) = refs::user(arg);
  let v = api
    .get(
      &format!("/contact/v3/users/{id}"),
      &[("user_id_type", kind.to_owned())],
    )
    .await?;
  Ok(parse::user(v.at("data.user")))
}

/// Fill in author names from the contact API, best effort: an app without
/// the contact scope keeps the bare open ids.
pub async fn names(api: &Api, posts: &mut [Post]) {
  let wanted: Vec<String> = {
    let known = api.names.borrow();
    let mut ids: Vec<String> = posts
      .iter()
      .filter_map(|p| p.author.as_ref())
      .map(|a| a.id.clone())
      .filter(|id| id.starts_with("ou_") && !known.contains_key(id))
      .collect();
    ids.sort();
    ids.dedup();
    ids
  };
  if !wanted.is_empty() && !api.contacts_denied.get() {
    for chunk in wanted.chunks(50) {
      let mut query = vec![("user_id_type", "open_id".to_owned())];
      query.extend(chunk.iter().map(|id| ("user_ids", id.clone())));
      match api.get("/contact/v3/users/batch", &query).await {
        Ok(v) => {
          let mut known = api.names.borrow_mut();
          for u in v.list("data.items") {
            if let (Some(id), Some(name)) = (u.str("open_id"), u.str("name")) {
              known.insert(id, name);
            }
          }
        }
        Err(e) => {
          tracing::debug!("feishu contact lookup failed: {e}");
          api.contacts_denied.set(true);
          break;
        }
      }
    }
  }
  let known = api.names.borrow();
  for p in posts {
    if let Some(a) = p.author.as_mut()
      && a.name.is_empty()
      && let Some(name) = known.get(&a.id)
    {
      a.name = name.clone();
    }
    if let Some(name) = p
      .extra
      .get("chat")
      .and_then(|c| c.as_str())
      .and_then(|c| known.get(c))
    {
      p.extra.insert("chat_name".into(), name.clone().into());
    }
  }
}

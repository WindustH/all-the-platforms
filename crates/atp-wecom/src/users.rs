//! The app or AI bot itself, and corp users.

use atp_core::{Result, User, Value, ValueExt};

use crate::api::Api;
use crate::parse;

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let id = arg.trim().trim_start_matches("user:");
  let v = api.get("/user/get", &[("userid", id.to_owned())]).await?;
  Ok(parse::user(id, &v))
}

/// The app (`agent/get`), the bare corp when no agent id is known, or the
/// AI bot when only it is logged in.
pub async fn whoami(api: &Api) -> Result<User> {
  if api.app().is_none()
    && let Some(bot) = api.bot()
  {
    return Ok(User {
      id: bot.bot_id.clone(),
      name: format!("WeCom AI bot {}", bot.bot_id),
      ..User::default()
    });
  }
  let app = api.require_app()?;
  api.token().await?;
  let Some(agent) = app.agent_id.clone() else {
    return Ok(User {
      id: app.corp_id.clone(),
      name: format!("WeCom corp {}", app.corp_id),
      ..User::default()
    });
  };
  let v = api.get("/agent/get", &[("agentid", agent.clone())]).await?;
  Ok(User {
    id: agent,
    name: v.str("name").unwrap_or_default(),
    avatar: v.str("square_logo_url"),
    bio: v.str("description").filter(|d| !d.is_empty()),
    extra: [("corp_id".to_owned(), Value::from(app.corp_id))]
      .into_iter()
      .collect(),
    raw: Some(v.clone()),
    ..User::default()
  })
}

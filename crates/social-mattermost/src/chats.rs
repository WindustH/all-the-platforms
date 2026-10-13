//! Teams and channels: `chats` lists the teams and your direct and group
//! channels, `chats TEAM` the team's channels you belong to; channel and
//! team arguments are resolved here, and members listed.

use std::collections::HashMap;

use social_core::{
  Collection, Error, ErrorCode, Page, PageReq, Result, User, Value, ValueExt, json,
};

use crate::api::Api;
use crate::parse::{self, View};
use crate::refs::{self, Chat};
use crate::users::{self, Brief};

/// A channel's name, kind and team, for post listings.
#[derive(Debug, Clone)]
pub struct ChannelInfo {
  pub name: String,
  pub kind: String,
  pub team: Option<String>,
}

pub fn info_of(v: &Value, me: &str, users: &HashMap<String, Brief>) -> ChannelInfo {
  let t = v.str("type").unwrap_or_default();
  let kind = match t.as_str() {
    "O" => "channel",
    "P" => "private",
    "D" => "dm",
    "G" => "group",
    _ => "channel",
  };
  let name = if kind == "dm" {
    let slug = v.str("name").unwrap_or_default();
    let other = slug
      .split("__")
      .find(|id| *id != me)
      .unwrap_or(me)
      .to_owned();
    users
      .get(&other)
      .map(|b| format!("@{}", b.handle))
      .unwrap_or(format!("@{other}"))
  } else {
    v.first_str(&["display_name", "name"]).unwrap_or_default()
  };
  ChannelInfo {
    name,
    kind: kind.into(),
    team: v.str("team_id").filter(|t| !t.is_empty()),
  }
}

/// Learn channel names (and the users of direct ones).
pub async fn remember(api: &Api, rows: &[Value]) {
  let me = api.me().await.unwrap_or_default();
  let dm_users = rows
    .iter()
    .filter(|c| c.str("type").as_deref() == Some("D"))
    .filter_map(|c| c.str("name"))
    .flat_map(|n| n.split("__").map(str::to_owned).collect::<Vec<_>>());
  users::resolve(api, dm_users).await;
  let known = api.users.borrow().clone();
  let mut channels = api.channels.borrow_mut();
  for c in rows {
    if let Some(id) = c.str("id") {
      channels.insert(id, info_of(c, &me, &known));
    }
  }
}

async fn my_teams(api: &Api) -> Result<Vec<Value>> {
  let v = api.get("/users/me/teams", &[]).await?;
  let rows = v.as_array().cloned().unwrap_or_default();
  let mut teams = api.teams.borrow_mut();
  for t in &rows {
    if let (Some(id), Some(name)) = (t.str("id"), t.str("name")) {
      teams.insert(id, name);
    }
  }
  Ok(rows)
}

/// Make sure team names are known (links need one).
pub async fn teams_known(api: &Api) {
  if api.teams.borrow().is_empty() {
    let _ = my_teams(api).await;
  }
}

/// Run `f` with what parsing needs.
pub async fn render<T>(api: &Api, f: impl FnOnce(&View) -> T) -> Result<T> {
  let creds = api.require()?;
  teams_known(api).await;
  let me = api.me().await?;
  let users = api.users.borrow();
  let channels = api.channels.borrow();
  let teams = api.teams.borrow();
  Ok(f(&View {
    server: &creds.server,
    token: &creds.token,
    me: &me,
    users: &users,
    channels: &channels,
    teams: &teams,
  }))
}

async fn team_channels(api: &Api, team: &str) -> Result<Vec<Value>> {
  let v = api
    .get(&format!("/users/me/teams/{team}/channels"), &[])
    .await?;
  let mut rows = v.as_array().cloned().unwrap_or_default();
  rows.sort_by_key(|c| std::cmp::Reverse(c.i64("last_post_at").unwrap_or_default()));
  remember(api, &rows).await;
  Ok(rows)
}

pub async fn list(api: &Api, parent: Option<&str>, _page: &PageReq) -> Result<Page<Collection>> {
  let server = api.server()?;
  let teams = my_teams(api).await?;
  let items = match parent {
    Some(arg) => {
      let team = team_id(api, arg).await?;
      let rows: Vec<Value> = team_channels(api, &team)
        .await?
        .into_iter()
        .filter(|c| matches!(c.str("type").as_deref(), Some("O" | "P")))
        .collect();
      render(api, |view| {
        rows.iter().map(|c| parse::channel(c, view)).collect()
      })
      .await?
    }
    None => {
      let mut items: Vec<Collection> = teams.iter().map(|t| parse::team(t, &server)).collect();
      if let Some(first) = teams.first().and_then(|t| t.str("id")) {
        let direct: Vec<Value> = team_channels(api, &first)
          .await?
          .into_iter()
          .filter(|c| matches!(c.str("type").as_deref(), Some("D" | "G")))
          .collect();
        items.extend(
          render(api, |view| {
            direct
              .iter()
              .map(|c| parse::channel(c, view))
              .collect::<Vec<_>>()
          })
          .await?,
        );
      }
      items
    }
  };
  Ok(Page::last(items))
}

/// A team id from an id or a name.
pub async fn team_id(api: &Api, arg: &str) -> Result<String> {
  let arg = arg.trim().trim_end_matches('/');
  let name = arg.rsplit('/').next().unwrap_or(arg);
  if refs::is_id(name) {
    return Ok(name.to_owned());
  }
  if let Some((id, _)) = api.teams.borrow().iter().find(|(_, n)| n.as_str() == name) {
    return Ok(id.clone());
  }
  let v = api.get(&format!("/teams/name/{name}"), &[]).await?;
  v.str("id")
    .ok_or_else(|| Error::not_found(format!("no team {arg}")))
}

/// The channel id an argument names.
pub async fn channel_id(api: &Api, arg: &str) -> Result<String> {
  let v = match refs::chat(arg) {
    Chat::Id(id) => return Ok(id),
    Chat::User(user) => {
      let me = api.me().await?;
      let other = users::user_id(api, &user).await?;
      api.post("/channels/direct", json!([me, other])).await?
    }
    Chat::Path(team, name) => {
      api
        .get(&format!("/teams/name/{team}/channels/name/{name}"), &[])
        .await?
    }
    Chat::Name(name) => {
      let mut found = None;
      for t in my_teams(api).await? {
        let Some(id) = t.str("id") else { continue };
        if let Ok(c) = api
          .get(&format!("/teams/{id}/channels/name/{name}"), &[])
          .await
        {
          found = Some(c);
          break;
        }
      }
      found.ok_or_else(|| {
        Error::not_found(format!(
          "no channel named {name} in your teams; `social mattermost chats TEAM` lists them"
        ))
      })?
    }
  };
  remember(api, std::slice::from_ref(&v)).await;
  v.str("id")
    .ok_or_else(|| Error::upstream("channel without id"))
}

/// Name and team of a channel, asked once.
pub async fn info(api: &Api, id: &str) {
  if api.channels.borrow().contains_key(id) {
    return;
  }
  if let Ok(v) = api.get(&format!("/channels/{id}"), &[]).await {
    remember(api, std::slice::from_ref(&v)).await;
  }
}

/// Members of a channel, or of a team when the argument is one.
pub async fn members(api: &Api, arg: &str, page: &PageReq) -> Result<Page<User>> {
  let per_page = page.size_within(200);
  let no = page.number_or(0);
  let query = [("page", no.to_string()), ("per_page", per_page.to_string())];
  let rows = match channel_id(api, arg).await {
    Ok(id) => api.get(&format!("/channels/{id}/members"), &query).await?,
    Err(e) if e.code == ErrorCode::NotFound => {
      let team = team_id(api, arg).await?;
      api.get(&format!("/teams/{team}/members"), &query).await?
    }
    Err(e) => return Err(e),
  };
  let ids: Vec<String> = rows
    .as_array()
    .into_iter()
    .flatten()
    .filter_map(|m| m.str("user_id"))
    .collect();
  let next = (ids.len() >= per_page).then(|| (no + 1).to_string());
  Ok(Page::new(users::many(api, &ids).await, next))
}

//! The bot's account and the people it reaches: profiles, groups, members
//! and friends.

use atp_core::{Error, Page, PageReq, Result, User, UserStats, Value, ValueExt, json};

use crate::api::Api;
use crate::parse;
use crate::refs::{self, ChatRef};

pub async fn whoami(api: &Api) -> Result<User> {
  let v = api.get("/v2/bot/info").await?;
  let mut u = User {
    id: v.str("userId").unwrap_or_default(),
    name: v.str("displayName").unwrap_or_default(),
    handle: v.str("basicId"),
    avatar: v.str("pictureUrl"),
    raw: Some(v.clone()),
    ..User::default()
  };
  for key in ["premiumId", "chatMode", "markAsReadMode"] {
    if let Some(x) = v.str(key) {
      u.extra.insert(key.into(), json!(x));
    }
  }
  if let Ok(n) = api.get("/v2/bot/insight/followers").await
    && let Some(f) = n.u64("followers")
  {
    u.stats = UserStats {
      followers: Some(f),
      ..UserStats::default()
    };
  }
  Ok(u)
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  match refs::chat(arg)? {
    ChatRef::User(id) => Ok(parse::profile(
      &api.get(&format!("/v2/bot/profile/{id}")).await?,
    )),
    ChatRef::Group(id) => {
      let v = api.get(&format!("/v2/bot/group/{id}/summary")).await?;
      let count = api
        .get(&format!("/v2/bot/group/{id}/members/count"))
        .await
        .ok()
        .and_then(|c| c.u64("count"));
      Ok(User {
        id,
        name: v.str("groupName").unwrap_or_default(),
        avatar: v.str("pictureUrl"),
        stats: UserStats {
          followers: count,
          ..UserStats::default()
        },
        raw: Some(v),
        ..User::default()
      })
    }
    _ => Err(Error::input("give a user (U…) or group (C…) id")),
  }
}

/// Member ids of a group or room (verified / premium accounts), with
/// profiles; `next` pages on.
pub async fn members(api: &Api, arg: &str, page: &PageReq) -> Result<Page<User>> {
  let (kind, id) = match refs::chat(arg)? {
    ChatRef::Group(id) => ("group", id),
    ChatRef::Room(id) => ("room", id),
    ChatRef::User(id) => {
      return Ok(Page::last(vec![
        whoami(api).await?,
        parse::profile(&api.get(&format!("/v2/bot/profile/{id}")).await?),
      ]));
    }
    _ => return Err(Error::input("members of a group (C…) or room (R…)")),
  };
  let mut path = format!("/v2/bot/{kind}/{id}/members/ids");
  if let Some(c) = &page.cursor {
    path.push_str(&format!("?start={c}"));
  }
  let v = api.get(&path).await?;
  let mut users = Vec::new();
  for uid in v.list("memberIds").iter().filter_map(Value::as_str) {
    let p = api
      .get(&format!("/v2/bot/{kind}/{id}/member/{uid}"))
      .await
      .map(|p| parse::profile(&p))
      .unwrap_or_else(|_| User {
        id: uid.to_owned(),
        ..User::default()
      });
    users.push(p);
    if users.len() >= page.size.max(1) {
      break;
    }
  }
  Ok(Page::new(users, v.str("next")))
}

/// Friends of the bot (verified / premium accounts), with profiles; LINE
/// lists those of your own bot only.
pub async fn followers(api: &Api, user: &str, page: &PageReq) -> Result<Page<User>> {
  if !matches!(user, "" | "me") && user != whoami(api).await?.id {
    return Err(
      Error::unsupported("followers of another account")
        .with_hint("LINE lists the friends of your own bot only"),
    );
  }
  let size = page.size_within(1000);
  let mut path = format!("/v2/bot/followers/ids?limit={size}");
  if let Some(c) = &page.cursor {
    path.push_str(&format!("&start={c}"));
  }
  let v = api.get(&path).await?;
  let mut users = Vec::new();
  for uid in v.list("userIds").iter().filter_map(Value::as_str) {
    users.push(
      api
        .get(&format!("/v2/bot/profile/{uid}"))
        .await
        .map(|p| parse::profile(&p))
        .unwrap_or_else(|_| User {
          id: uid.to_owned(),
          ..User::default()
        }),
    );
  }
  Ok(Page::new(users, v.str("next")))
}

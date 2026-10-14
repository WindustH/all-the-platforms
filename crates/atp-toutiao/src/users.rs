//! Users: profiles and posts come from the signed user feed (its cells carry
//! the author), follower counts from `fans_stat`; 微头条 of a user also come
//! from the legacy feed, which needs no signature.

use atp_core::{Error, ErrorCode, Page, PageReq, Post, Result, User, Value, ValueExt};

use crate::api::{self, Api};
use crate::feeds::more;
use crate::parse;
use crate::refs;

pub async fn whoami(api: &Api) -> Result<User> {
  api.require_login()?;
  let v = api.get("/api/pc/user/info", &[]).await?;
  let data = v.at("data");
  let name = data
    .first_str(&["name", "screen_name", "user_name"])
    .ok_or_else(|| Error::auth("Toutiao did not recognize the session"))?;
  Ok(User {
    id: data
      .first_str(&["user_id", "id", "user_id_str"])
      .unwrap_or_default(),
    name,
    avatar: data.first_str(&["avatar_url", "avatar"]),
    bio: data.str("description").filter(|d| !d.is_empty()),
    raw: Some(data.clone()),
    ..User::default()
  })
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let token = refs::user_token(arg)?;
  let page = user_feed(api, &token, None).await?;
  let mut user = page
    .0
    .iter()
    .find_map(|c| parse::user(c.at("user_info")))
    .ok_or_else(|| Error::not_found(format!("no posts and no profile for {token}")))?;
  user.id = token.clone();
  user.url = Some(refs::user_url(&token));
  user.stats.posts = None;
  if let Ok(v) = fans_stat(api, &token).await {
    user.stats.followers = v.count("data.fans");
    user.stats.likes = v.count("data.digg_count");
  }
  Ok(user)
}

/// A user's posts: the signed user feed, or the unsigned 微头条 feed when
/// the signed one is refused.
pub async fn posts(api: &Api, arg: &str, req: &PageReq) -> Result<Page<Post>> {
  let token = refs::user_token(arg)?;
  match user_feed(api, &token, req.cursor.as_deref()).await {
    Ok((cells, next)) => Ok(Page::new(
      cells.iter().filter_map(parse::post).collect(),
      next,
    )),
    Err(e) if e.code == ErrorCode::VerificationRequired && req.cursor.is_none() => {
      threads(api, &token).await
    }
    Err(e) => Err(e),
  }
}

/// One page of the signed user feed: its cells and the next cursor (ms).
async fn user_feed(
  api: &Api,
  token: &str,
  cursor: Option<&str>,
) -> Result<(Vec<Value>, Option<String>)> {
  let query = [
    ("category", "profile_all".to_owned()),
    ("token", token.to_owned()),
    ("max_behot_time", cursor.unwrap_or("0").to_owned()),
    ("entrance_gid", String::new()),
  ];
  let v = api::check(
    api
      .signed(api::GET, "/api/pc/list/user/feed", &query, &[])
      .await?,
  )?;
  let next = more(&v)
    .then(|| v.i64("next.max_behot_time").filter(|t| *t > 0))
    .flatten()
    .map(|t| t.to_string());
  Ok((v.list("data").to_vec(), next))
}

/// The user's 微头条 from the legacy feed (one page).
async fn threads(api: &Api, token: &str) -> Result<Page<Post>> {
  let query = [
    ("category", "pc_profile_ugc".to_owned()),
    ("utm_source", "toutiao".into()),
    ("visit_user_token", token.to_owned()),
    ("max_behot_time", "0".into()),
  ];
  let v = api::check(api.get("/api/pc/feed/", &query).await?)?;
  Ok(Page::last(
    v.list("data")
      .iter()
      .filter_map(parse::thread_post)
      .collect(),
  ))
}

/// Follower and like totals (signed over the form body).
async fn fans_stat(api: &Api, token: &str) -> Result<Value> {
  api
    .signed(
      api::POST,
      "/api/pc/user/fans_stat",
      &[],
      &[("token", token.to_owned())],
    )
    .await
}

/// Accounts a user follows (unverified: from a 2024 reference).
pub async fn following(api: &Api, arg: &str, req: &PageReq) -> Result<Page<User>> {
  let token = refs::user_token(arg)?;
  let cursor = req.cursor.clone().unwrap_or_else(|| "0".into());
  let query = [
    ("cursor", cursor),
    ("count", req.size_within(20).to_string()),
    ("token", token),
  ];
  let v = api::check(api.get("/api/pc/user/following", &query).await?)?;
  let users = v.list("data").iter().filter_map(parse::user).collect();
  let next = more(&v).then(|| v.str("cursor")).flatten();
  Ok(Page::new(users, next))
}

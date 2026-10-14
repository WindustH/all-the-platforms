//! Users, their collections (收藏), and changes to your own collection:
//! state, rating, comment and tags of a subject, and how far you are.

use atp_core::http::Method;
use atp_core::{
  Action, Error, Page, PageReq, Post, Result, User, Value, ValueExt, credentials, json,
};

use crate::api::{Api, TOKEN};
use crate::parse::{self, STATUSES, TYPES};
use crate::refs;

/// The most rows a collection listing returns at once.
const PAGE_MAX: usize = 50;
/// The most episodes one request lists.
const EPISODES_MAX: usize = 200;

pub async fn whoami(api: &Api) -> Result<User> {
  api.require()?;
  Ok(parse::user(&api.get("/v0/me", Vec::new()).await?))
}

/// Keep the token; `whoami` checks it right after.
pub fn login(api: &Api, token: &str) -> Result<()> {
  let token = token.trim();
  if token.is_empty() || token.contains(char::is_whitespace) {
    return Err(Error::input(
      "expected `login --token TOKEN` (a Bangumi personal access token)",
    ));
  }
  credentials::save(&api.ctx, &[(TOKEN, token)]);
  Ok(())
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let name = refs::user(arg)?;
  Ok(parse::user(
    &api.get(&format!("/v0/users/{name}"), Vec::new()).await?,
  ))
}

/// A user name, or yours when none is given.
async fn name_or_me(api: &Api, user: Option<&str>) -> Result<String> {
  match user {
    Some(user) => refs::user(user),
    None => Ok(whoami(api).await?.id),
  }
}

fn named(table: &[(u64, &str)], what: &str, name: &str) -> Result<u64> {
  parse::code(table, name).ok_or_else(|| Error::input(format!("unknown {what}: {name}")))
}

/// A user's collection, the most recently changed first.
pub async fn library(
  api: &Api,
  user: Option<&str>,
  kind: Option<&str>,
  status: Option<&str>,
  page: &PageReq,
) -> Result<Page<Post>> {
  let name = name_or_me(api, user).await?;
  let offset = page.number_or(0);
  let mut params = vec![
    ("limit", page.size_within(PAGE_MAX).to_string()),
    ("offset", offset.to_string()),
  ];
  if let Some(kind) = kind {
    params.push((
      "subject_type",
      named(TYPES, "subject type", kind)?.to_string(),
    ));
  }
  if let Some(status) = status {
    params.push(("type", named(STATUSES, "status", status)?.to_string()));
  }
  let v = api
    .get(&format!("/v0/users/{name}/collections"), params)
    .await?;
  let rows = v.list("data");
  let next = offset + rows.len() as u64;
  let more = !rows.is_empty() && v.u64("total").is_some_and(|total| next < total);
  Ok(Page::new(
    rows.iter().map(parse::collected).collect(),
    more.then(|| next.to_string()),
  ))
}

/// What `mark` changes; `None` leaves a field as it is.
pub struct Mark {
  pub status: String,
  pub rating: Option<u8>,
  pub comment: Option<String>,
  pub tags: Vec<String>,
  pub private: bool,
}

/// Add a subject to your collection, or change its entry.
pub async fn mark(api: &Api, arg: &str, mark: Mark) -> Result<Action> {
  let id = refs::subject(arg)?;
  let mut body = json!({"type": named(STATUSES, "status", &mark.status)?});
  if let Some(rating) = mark.rating {
    if rating > 10 {
      return Err(Error::input(
        "--rating takes 0 to 10 (0 removes the rating)",
      ));
    }
    body["rate"] = json!(rating);
  }
  if let Some(comment) = mark.comment {
    body["comment"] = json!(comment);
  }
  if !mark.tags.is_empty() {
    if mark
      .tags
      .iter()
      .any(|tag| tag.contains(char::is_whitespace))
    {
      return Err(Error::input("tags cannot contain spaces"));
    }
    body["tags"] = json!(mark.tags);
  }
  if mark.private {
    body["private"] = json!(true);
  }
  let path = format!("/v0/users/-/collections/{id}");
  api.write(Method::POST, &path, body).await?;
  Ok(Action::done(mark.status, arg))
}

/// How far you are: watched main episodes up to `episode` (anime, real),
/// or chapters and volumes read (books).
pub async fn progress(
  api: &Api,
  arg: &str,
  episode: Option<u32>,
  volume: Option<u32>,
) -> Result<Action> {
  let id = refs::subject(arg)?;
  api.require()?;
  let subject = api.get(&format!("/v0/subjects/{id}"), Vec::new()).await?;
  let path = format!("/v0/users/-/collections/{id}");
  if subject.u64("type") == Some(1) {
    let mut body = json!({});
    if let Some(chapters) = episode {
      body["ep_status"] = json!(chapters);
    }
    if let Some(volumes) = volume {
      body["vol_status"] = json!(volumes);
    }
    if body.as_object().is_some_and(|map| map.is_empty()) {
      return Err(Error::input("give the chapters read, --vol N, or both"));
    }
    api.write(Method::PATCH, &path, body).await?;
    return Ok(Action::done("progress", arg));
  }
  if volume.is_some() {
    return Err(Error::input("--vol applies to books only"));
  }
  let Some(upto) = episode else {
    return Err(Error::input("give the last episode you watched"));
  };
  let ids = main_episodes(api, id, upto).await?;
  if ids.is_empty() {
    return Err(Error::not_found(format!(
      "subject {id} lists no episodes up to {upto}"
    )));
  }
  let count = ids.len();
  let body = json!({"episode_id": ids, "type": 2});
  api
    .write(Method::PATCH, &format!("{path}/episodes"), body)
    .await?;
  Ok(Action::done("progress", arg).with_message(format!("{count} episodes marked watched")))
}

/// Ids of the main episodes numbered 1 to `upto`.
async fn main_episodes(api: &Api, subject: u64, upto: u32) -> Result<Vec<u64>> {
  let mut ids = Vec::new();
  let mut offset = 0;
  loop {
    let params = vec![
      ("subject_id", subject.to_string()),
      ("type", "0".to_owned()),
      ("limit", EPISODES_MAX.to_string()),
      ("offset", offset.to_string()),
    ];
    let v = api.get("/v0/episodes", params).await?;
    let rows = v.list("data");
    ids.extend(
      rows
        .iter()
        .filter(|row| number(row).is_some_and(|n| n <= upto as f64))
        .filter_map(|row| row.u64("id")),
    );
    offset += rows.len();
    let total = v.u64("total").unwrap_or(0) as usize;
    if rows.is_empty() || offset >= total {
      return Ok(ids);
    }
  }
}

/// An episode's number in its subject (`ep`), else its overall order.
fn number(v: &Value) -> Option<f64> {
  v.f64("ep").filter(|n| *n > 0.0).or_else(|| v.f64("sort"))
}

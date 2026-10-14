//! Users, what they marked (想看 / 看过 / 读过 …), and marking a subject
//! yourself.

use atp_core::{Action, Error, Page, PageReq, Post, Result, User, Value, ValueExt};

use crate::api::Api;
use crate::parse;
use crate::refs;
use crate::subjects::{next_start, page_of, subject_json};

/// `library --type`: the kinds of marks a user keeps.
pub const KINDS: &[&str] = &["movie", "book", "music"];
/// `--status` names and Douban's own.
const STATUSES: &[(&str, &str)] = &[("wish", "mark"), ("doing", "doing"), ("done", "done")];
/// The most marks one request lists.
const PAGE_MAX: usize = 50;

pub async fn whoami(api: &Api) -> Result<User> {
  api.require_login()?;
  let v = api.get("/~me", "/mine/", &[]).await?;
  if v.str("id").is_some() {
    return Ok(parse::user(&v));
  }
  let id = api
    .my_id()
    .ok_or_else(|| Error::auth("Douban did not recognize the session"))?;
  user(api, &id).await
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let name = refs::user(arg)?;
  let v = api
    .get(&format!("/user/{name}"), &format!("/people/{name}/"), &[])
    .await?;
  Ok(parse::user(&v))
}

/// A user, or the logged-in account when none is given.
async fn name_or_me(api: &Api, user: Option<&str>) -> Result<String> {
  match user {
    Some(user) => refs::user(user),
    None => {
      api.require_login()?;
      api
        .my_id()
        .ok_or_else(|| Error::auth("Douban did not recognize the session"))
    }
  }
}

fn douban_status(name: &str) -> Result<&'static str> {
  STATUSES
    .iter()
    .find(|(n, _)| *n == name)
    .map(|(_, s)| *s)
    .ok_or_else(|| Error::input(format!("unknown status: {name} (wish, doing or done)")))
}

/// What a user marked, the latest first.
pub async fn library(
  api: &Api,
  user: Option<&str>,
  kind: &str,
  status: &str,
  page: &PageReq,
) -> Result<Page<Post>> {
  if !KINDS.contains(&kind) {
    return Err(Error::input(format!(
      "unknown type: {kind} (movie, book or music)"
    )));
  }
  let name = name_or_me(api, user).await?;
  let start = page.number_or(0);
  let params = [
    ("type", kind.to_owned()),
    ("status", douban_status(status)?.to_owned()),
    ("start", start.to_string()),
    ("count", page.size_within(PAGE_MAX).to_string()),
  ];
  let path = format!("/user/{name}/interests");
  let v = api
    .get(&path, &format!("/people/{name}/{kind}/"), &params)
    .await?;
  let rows = v.list("interests");
  let items = rows.iter().map(parse::collected).collect();
  Ok(Page::new(items, next_start(&v, start, rows.len())))
}

/// Mark a subject 想看 (`wish`), 在看 / 在读 (`doing`) or 看过 / 读过
/// (`done`), with stars (1–5) for the last two, as the subject page's form
/// does. An existing short comment and tags are kept.
pub async fn mark(api: &Api, arg: &str, status: &str, rating: Option<u8>) -> Result<Action> {
  api.require_login()?;
  let mut subject = refs::subject_ref(arg)?;
  let state = douban_status(status)?;
  if rating.is_some_and(|r| !(1..=5).contains(&r)) {
    return Err(Error::input("--rating takes 1 to 5 stars"));
  }
  if rating.is_some() && state == "mark" {
    return Err(Error::input("`wish` takes no rating"));
  }
  let current = subject_json(api, &subject).await?;
  if let Some(kind) = current.str("type") {
    subject.kind = kind;
  }
  if state == "doing" && subject.kind == "movie" {
    return Err(Error::input("films can only be `wish` or `done`"));
  }
  let form = mark_form(current.at("interest"), state, rating);
  let path = format!("/{}/{}/{state}", subject.kind, subject.id);
  api.post(&path, &page_of(&subject), &form).await?;
  Ok(
    Action::done(status, arg).with_url(
      current
        .str("url")
        .unwrap_or_else(|| format!("https://movie.douban.com/subject/{}/", subject.id)),
    ),
  )
}

/// The form fields, keeping what the existing mark says.
fn mark_form(mine: &Value, state: &str, rating: Option<u8>) -> Vec<(&'static str, String)> {
  let tags: Vec<&str> = mine.list("tags").iter().filter_map(Value::as_str).collect();
  let mut form = vec![
    ("tags", tags.join(" ")),
    ("comment", mine.str("comment").unwrap_or_default()),
    ("sync_douban", "0".to_owned()),
  ];
  let stars = rating.map(u64::from).or_else(|| mine.u64("rating.value"));
  if state != "mark"
    && let Some(stars) = stars
  {
    form.push(("rating", stars.to_string()));
  }
  form
}

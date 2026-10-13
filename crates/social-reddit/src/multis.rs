//! Multireddits: named groups of subreddits, browsed like one subreddit.

use social_core::{Collection, Error, Page, PageReq, Post, Result, Value};

use crate::api::Api;
use crate::refs::{self};
use crate::{listings, parse};

/// Your multireddits, or the public ones of `user`.
pub async fn list(api: &Api, user: Option<&str>) -> Result<Page<Collection>> {
  let path = match user {
    Some(u) => format!("/api/multi/user/{}", refs::user(u)?),
    None => {
      api.require_login()?;
      "/api/multi/mine".to_owned()
    }
  };
  let v = api.get(&path, &[("expand_srs", "true".to_owned())]).await?;
  let items = match &v {
    Value::Array(a) => a.as_slice(),
    _ => &[],
  };
  Ok(Page::last(
    items.iter().filter_map(parse::multireddit).collect(),
  ))
}

/// `/user/<name>/m/<multi>` from a link, that path, `u/name/m/multi` or `name/multi`.
fn path(arg: &str) -> Result<String> {
  let s = arg.trim();
  let s = s
    .find("reddit.com/")
    .map_or(s, |i| &s[i + "reddit.com".len()..]);
  let parts: Vec<&str> = s.split('/').filter(|p| !p.is_empty()).collect();
  let (user, name) = match parts.as_slice() {
    ["user" | "u", user, "m", name, ..] => (*user, *name),
    [user, name] => (*user, *name),
    _ => {
      return Err(Error::input(format!(
        "not a multireddit: {arg} (expected u/USER/m/NAME or a link)"
      )));
    }
  };
  Ok(format!("/user/{user}/m/{name}"))
}

/// Posts of a multireddit in `sort` order.
pub async fn posts(
  api: &Api,
  arg: &str,
  sort: &str,
  time: Option<&str>,
  req: &PageReq,
) -> Result<Page<Post>> {
  let base = path(arg)?;
  let query = time.map(|t| ("t", t.to_owned())).into_iter().collect();
  listings::posts(api, &format!("{base}/{sort}"), query, req).await
}

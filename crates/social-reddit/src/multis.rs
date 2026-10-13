//! Multireddits: named groups of subreddits, browsed like one subreddit.

use social_core::text::from_secs;
use social_core::{Collection, Error, Page, PageReq, Post, Result, User, Value, ValueExt};

use crate::api::{Api, WWW};
use crate::listing;
use crate::refs::{self, user_url};

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
  Ok(Page::last(items.iter().filter_map(multi).collect()))
}

fn multi(thing: &Value) -> Option<Collection> {
  let d = thing.at("data");
  let path = d.str("path")?;
  let subs: Vec<Value> = d
    .list("subreddits")
    .iter()
    .filter_map(|s| s.str("name"))
    .map(Value::from)
    .collect();
  let owner = d.str("owner").map(|o| User {
    id: o.clone(),
    url: Some(user_url(&o)),
    handle: Some(o.clone()),
    name: o,
    ..User::default()
  });
  let mut c = Collection {
    id: path.trim_end_matches('/').to_owned(),
    kind: "multireddit".into(),
    name: d.first_str(&["display_name", "name"]).unwrap_or_default(),
    description: d.str("description_md").filter(|s| !s.is_empty()),
    url: Some(format!("{WWW}{path}")),
    items: Some(subs.len() as u64),
    followers: d.u64("num_subscribers"),
    owner,
    ..Collection::default()
  };
  c.extra.insert("subreddits".into(), Value::from(subs));
  if let Some(v) = d.str("visibility") {
    c.extra.insert("visibility".into(), v.into());
  }
  if let Some(t) = d.i64("created_utc").and_then(from_secs) {
    c.extra.insert("created_at".into(), t.to_string().into());
  }
  Some(c)
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
  listing::posts(api, &format!("{base}/{sort}"), query, req).await
}

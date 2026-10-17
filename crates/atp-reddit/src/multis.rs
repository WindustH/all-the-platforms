//! Multireddits (custom feeds): named groups of subreddits, browsed like one
//! subreddit; yours are created, changed, filled and deleted through
//! `/api/multi/<multipath>` (POST, PUT, DELETE and `…/r/<subreddit>`).

use atp_core::bulk;
use atp_core::{Action, Collection, Error, Page, PageReq, Post, Result, Value, json};

use crate::api::{Api, WWW};
use crate::refs::{self};
use crate::{listings, parse};

/// Who sees a multireddit.
pub const VISIBILITY: &[&str] = &["private", "public", "hidden"];

/// Longest name in a new multireddit's path, as far as known.
const NAME_MAX: usize = 21;

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

/// What `multi-create` and `multi-update` set; `None` keeps a field.
#[derive(Debug, Default)]
pub struct MultiSpec<'a> {
  /// The display name.
  pub name: Option<&'a str>,
  pub description: Option<&'a str>,
  /// One of [`VISIBILITY`].
  pub visibility: Option<&'a str>,
}

/// The name in a new multireddit's path, from its display name: ASCII
/// letters and digits, `_` for the rest.
fn slug(name: &str) -> Result<String> {
  let mapped: String = name
    .chars()
    .map(|c| {
      if c.is_ascii_alphanumeric() {
        c.to_ascii_lowercase()
      } else {
        '_'
      }
    })
    .collect();
  let mut slug = String::new();
  for part in mapped.split('_').filter(|p| !p.is_empty()) {
    if !slug.is_empty() {
      slug.push('_');
    }
    slug.push_str(part);
  }
  slug.truncate(NAME_MAX);
  let slug = slug.trim_end_matches('_').to_owned();
  if slug.len() < 2 {
    return Err(Error::input(format!(
      "`{name}` gives no name for the multireddit's link: use at least two ASCII letters or digits"
    )));
  }
  Ok(slug)
}

fn model(spec: &MultiSpec<'_>, subreddits: &[String]) -> String {
  let subs: Vec<Value> = subreddits.iter().map(|s| json!({ "name": s })).collect();
  json!({
    "display_name": spec.name.unwrap_or_default(),
    "description_md": spec.description.unwrap_or_default(),
    "visibility": spec.visibility.unwrap_or("private"),
    "subreddits": subs,
  })
  .to_string()
}

fn subreddits(args: &[String]) -> Result<Vec<String>> {
  args.iter().map(|s| refs::subreddit(s)).collect()
}

/// Create a multireddit of the logged-in account, private unless the spec
/// says otherwise, holding `subs`.
pub async fn create(api: &Api, spec: &MultiSpec<'_>, subs: &[String]) -> Result<Action> {
  let name = spec.name.unwrap_or_default();
  let path = format!("/user/{}/m/{}", api.username().await?, slug(name)?);
  let form = vec![
    ("model", model(spec, &subreddits(subs)?)),
    ("multipath", path.clone()),
  ];
  api.post(&format!("/api/multi{path}"), form).await?;
  Ok(
    Action::done("multi-create", name)
      .with_id(&path)
      .with_url(format!("{WWW}{path}")),
  )
}

/// Change a multireddit's name, description or visibility. The update
/// takes the whole multireddit, so the rest (its subreddits among them) is
/// read from your multireddits and sent back as it was.
pub async fn update(api: &Api, arg: &str, change: &MultiSpec<'_>) -> Result<Action> {
  if change.name.is_none() && change.description.is_none() && change.visibility.is_none() {
    return Err(Error::input(
      "nothing to change: give --name, --desc or --visibility",
    ));
  }
  let path = path(arg)?;
  let mine = list(api, None).await?;
  let multi = mine
    .items
    .iter()
    .find(|m| m.id.eq_ignore_ascii_case(&path))
    .ok_or_else(|| Error::not_found(format!("{path} is none of your multireddits")))?;
  let kept: Vec<String> = multi
    .extra
    .get("subreddits")
    .and_then(Value::as_array)
    .into_iter()
    .flatten()
    .filter_map(|s| s.as_str().map(str::to_owned))
    .collect();
  let visibility = multi.extra.get("visibility").and_then(Value::as_str);
  let spec = MultiSpec {
    name: change.name.or(Some(multi.name.as_str())),
    description: change.description.or(multi.description.as_deref()),
    visibility: change.visibility.or(visibility),
  };
  let form = vec![
    ("model", model(&spec, &kept)),
    ("multipath", multi.id.clone()),
  ];
  api.put(&format!("/api/multi{}", multi.id), form).await?;
  Ok(Action::done("multi-update", &multi.id).with_url(format!("{WWW}{}", multi.id)))
}

pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  let path = path(arg)?;
  api
    .delete(
      &format!("/api/multi{path}"),
      vec![("multipath", path.clone())],
    )
    .await?;
  Ok(Action::done("multi-delete", path))
}

/// Add subreddits to a multireddit, or take them out with `undo`, one call
/// each.
pub async fn file(api: &Api, arg: &str, subs: &[String], undo: bool) -> Result<Vec<Action>> {
  let path = path(arg)?;
  let names = subreddits(subs)?;
  let action = if undo { "multi-remove" } else { "multi-add" };
  bulk::each(action, &names, async |name| {
    let url = format!("/api/multi{path}/r/{name}");
    let mut form = vec![("multipath", path.clone()), ("srname", name.to_owned())];
    if undo {
      api.delete(&url, form).await?;
    } else {
      form.push(("model", json!({ "name": name }).to_string()));
      api.put(&url, form).await?;
    }
    Ok(
      Action::done(action, format!("r/{name}"))
        .with_url(format!("{WWW}{path}"))
        .with_message(format!("multireddit {path}")),
    )
  })
  .await
}

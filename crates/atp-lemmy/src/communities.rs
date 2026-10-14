//! Communities: one community, lists of them, subscribing.

use atp_core::{Action, Collection, Error, Page, PageReq, Result, ValueExt, json};

use crate::api::Api;
use crate::parse;
use crate::refs;

/// `communities --type`.
pub const LISTINGS: &[&str] = &["all", "local", "subscribed"];
/// `communities --sort`.
pub const SORTS: &[&str] = &[
  "active",
  "hot",
  "new",
  "old",
  "top-day",
  "top-week",
  "top-month",
  "top-year",
  "top-all",
];
const PAGE_MAX: usize = 50;

pub async fn info(api: &Api, arg: &str) -> Result<Collection> {
  let name = refs::community(arg, &api.instance())?;
  let v = api.get("community", vec![("name", name)]).await?;
  Ok(parse::community(v.at("community_view")))
}

/// The instance's communities (all it knows, its own, or yours).
pub async fn list(
  api: &Api,
  listing: &str,
  order: &str,
  page: &PageReq,
) -> Result<Page<Collection>> {
  if !LISTINGS.contains(&listing) {
    return Err(Error::input(format!("unknown listing: {listing}")));
  }
  if !SORTS.contains(&order) {
    return Err(Error::input(format!("unknown sort: {order}")));
  }
  if listing == "subscribed" {
    api.require()?;
  }
  let number = page.number_or(1);
  let limit = page.size_within(PAGE_MAX);
  let params = vec![
    ("type_", capital(listing)),
    ("sort", crate::posts::sort(Some(order), "hot")?),
    ("page", number.to_string()),
    ("limit", limit.to_string()),
  ];
  let v = api.get("community/list", params).await?;
  let rows = v.list("communities");
  let next = (rows.len() >= limit).then(|| (number + 1).to_string());
  Ok(Page::new(rows.iter().map(parse::community).collect(), next))
}

/// `all` → `All`.
fn capital(word: &str) -> String {
  let mut chars = word.chars();
  chars
    .next()
    .map(|first| first.to_ascii_uppercase().to_string() + chars.as_str())
    .unwrap_or_default()
}

pub async fn subscribe(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  api.require()?;
  let community = info(api, arg).await?;
  let id = community
    .raw
    .as_ref()
    .and_then(|v| v.u64("community.id"))
    .ok_or_else(|| Error::not_found(format!("no community {arg}")))?;
  let body = json!({"community_id": id, "follow": !undo});
  api.write("community/follow", body).await?;
  Ok(Action::done(
    if undo { "unsubscribe" } else { "subscribe" },
    community.id,
  ))
}

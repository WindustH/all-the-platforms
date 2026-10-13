//! Boards (专辑): the named collections notes are saved into, and their notes.
//!
//! Few accounts still have boards; the web app lists them on profiles
//! (`board/user`, page-numbered) and opens one at `/board/<id>`.

use std::sync::LazyLock;

use atp_core::{Collection, Error, Page, PageReq, Post, Result, Value, ValueExt};
use regex::Regex;

use crate::api::Api;
use crate::refs;
use crate::users::{self, note_page};

const USER_BOARDS: &str = "/api/sns/web/v1/board/user";
const BOARD_NOTES: &str = "/api/sns/web/v1/board/note";

static BOARD_LINK: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"/board/([0-9a-f]{24})").expect("regex"));

/// A user's boards (default: the logged-in account).
pub async fn list(api: &Api, user: Option<&str>, req: &PageReq) -> Result<Page<Collection>> {
  api.require_login()?;
  let id = match user {
    Some(arg) => refs::user_ref(api, arg).await?,
    None => users::whoami(api).await?.id,
  };
  let page = req.number_or(1);
  let (page_s, num) = (page.to_string(), req.size_within(15).to_string());
  let params = [
    ("user_id", id.as_str()),
    ("num", num.as_str()),
    ("page", page_s.as_str()),
  ];
  let data = api.get(USER_BOARDS, &params).await?;
  let items: Vec<Collection> = data.list("boards").iter().filter_map(board).collect();
  let next =
    (!items.is_empty() && data.bool("has_more") != Some(false)).then(|| (page + 1).to_string());
  Ok(Page::new(items, next))
}

/// Notes of one board, by id or `/board/<id>` link.
pub async fn notes(api: &Api, arg: &str, req: &PageReq) -> Result<Page<Post>> {
  api.require_login()?;
  let id = board_id(arg)?;
  let num = req.size_within(30).to_string();
  let cursor = req.cursor.clone().unwrap_or_default();
  let params = [
    ("board_id", id.as_str()),
    ("num", num.as_str()),
    ("cursor", cursor.as_str()),
    ("image_formats", "jpg,webp,avif"),
  ];
  Ok(note_page(api, &api.get(BOARD_NOTES, &params).await?))
}

fn board_id(arg: &str) -> Result<String> {
  let arg = arg.trim();
  if let Some(m) = BOARD_LINK.captures(arg) {
    return Ok(m[1].to_owned());
  }
  if arg.len() == 24 && arg.bytes().all(|b| b.is_ascii_hexdigit()) {
    return Ok(arg.to_owned());
  }
  Err(Error::input(format!(
    "not a board: {arg} (expected a board id or a /board/<id> link)"
  )))
}

fn board(v: &Value) -> Option<Collection> {
  let id = v.first_str(&["id", "board_id"])?;
  let cover = v
    .list("images")
    .first()
    .and_then(|i| i.as_str().map(str::to_owned).or_else(|| i.str("url")));
  let mut collection = Collection {
    url: Some(format!("{}/board/{id}", crate::api::HOME)),
    kind: "board".into(),
    name: v.first_str(&["name", "title"]).unwrap_or_default(),
    description: v.str("desc"),
    items: v.first_count(&["total", "note_count", "notes_count"]),
    followers: v.first_count(&["fans", "fans_count", "subscribe_count"]),
    raw: Some(v.clone()),
    id,
    ..Collection::default()
  };
  for (key, value) in [
    ("cover", cover.map(Value::from)),
    ("private", v.get("privacy").cloned()),
  ] {
    if let Some(value) = value.filter(|v| !v.is_null()) {
      collection.extra.insert(key.into(), value);
    }
  }
  Some(collection)
}

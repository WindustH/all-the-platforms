//! Boards (专辑): the named collections notes are saved into, their notes,
//! and managing them as the profile's 专辑 tab and the board page do.
//!
//! Few accounts still have boards; the web app lists them on profiles
//! (`board/user`, page-numbered) and opens one at `/board/<id>`. It puts a
//! collected note into a board with `note/move`, which names only the board
//! the note goes to: as far as the web shows, a note sits in one board at a
//! time, and nothing takes it out of one without uncollecting it.

use atp_core::{
  Action, Collection, Error, FolderSpec, Page, PageReq, Post, Result, Value, ValueExt, bulk, json,
};

use crate::api::Api;
use crate::refs::{self, NoteRef, board_ref, board_url};
use crate::users::{self, note_page};

const BOARD: &str = "/api/sns/web/v1/board";
const USER_BOARDS: &str = "/api/sns/web/v1/board/user";
const BOARD_NOTES: &str = "/api/sns/web/v1/board/note";
const MOVE: &str = "/api/sns/web/v1/note/move";

/// The longest name and description the web's board form takes.
const NAME_MAX: usize = 12;
const DESC_MAX: usize = 1000;

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
  let id = board_ref(arg)?;
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

// ── managing ────────────────────────────────────────────────────────────

/// Create a board, public unless `private`, as the 专辑 tab's form does.
pub async fn create(api: &Api, spec: &FolderSpec) -> Result<Action> {
  api.require_login()?;
  check(spec)?;
  let name = spec.name.clone().unwrap_or_default();
  let body = json!({
    "name": name,
    "desc": spec.description.as_deref().unwrap_or_default(),
    "privacy": privacy(spec.private.unwrap_or(false)),
  });
  api.write_pause().await;
  let data = api.post(BOARD, &body).await?;
  let action = Action::done("folder-create", name);
  Ok(match data.first_str(&["id", "board_id"]) {
    Some(id) => action.with_url(board_url(&id)).with_id(id),
    None => action,
  })
}

/// Rename a board or change its description or privacy. The board page's
/// form sends all three, so what is not given is read from the board.
pub async fn edit(api: &Api, arg: &str, spec: &FolderSpec) -> Result<Action> {
  api.require_login()?;
  check(spec)?;
  let id = board_ref(arg)?;
  let board = info(api, &id).await?;
  let kept =
    |new: &Option<String>, key: &str| new.clone().or_else(|| board.str(key)).unwrap_or_default();
  let current = board
    .i64("privacy")
    .or_else(|| board.bool("privacy").map(i64::from));
  let body = json!({
    "boardid": id,
    "name": kept(&spec.name, "name"),
    "desc": kept(&spec.description, "desc"),
    "privacy": spec.private.map_or(current.unwrap_or(0), privacy),
  });
  api.write_pause().await;
  api.put(BOARD, &body).await?;
  Ok(Action::done("folder-edit", &id).with_url(board_url(&id)))
}

/// Delete boards, one call each. Their notes stay collected.
pub async fn delete(api: &Api, boards: &[String]) -> Result<Vec<Action>> {
  api.require_login()?;
  bulk::each("folder-delete", boards, async |arg| {
    let id = board_ref(arg)?;
    api.write_pause().await;
    api.delete(BOARD, &[("board_id", id.as_str())]).await?;
    Ok(Action::done("folder-delete", id))
  })
  .await
}

/// Put notes into a board, one call each; a note in another board leaves
/// it.
pub async fn add(api: &Api, board: &str, notes: &[String]) -> Result<Vec<Action>> {
  api.require_login()?;
  let id = board_ref(board)?;
  bulk::each("folder-add", notes, async |arg| {
    let note = place_ref(api, &id, arg).await?;
    let action = Action::done("folder-add", &note.id).with_url(note.url());
    Ok(action.with_message(format!("board {id}")))
  })
  .await
}

/// Move notes into another board with the same call, which does not name
/// the board they leave. A note sits in one board, so there is no `copy`.
pub async fn shift(
  api: &Api,
  from: &str,
  to: &str,
  notes: &[String],
  copy: bool,
) -> Result<Vec<Action>> {
  if copy {
    return Err(
      Error::unsupported("folder-move --copy")
        .with_hint("a Xiaohongshu note sits in one board at a time"),
    );
  }
  api.require_login()?;
  let (from, to) = (board_ref(from)?, board_ref(to)?);
  bulk::each("folder-move", notes, async |arg| {
    let note = place_ref(api, &to, arg).await?;
    let action = Action::done("folder-move", &note.id).with_url(note.url());
    Ok(action.with_message(format!("{from} → {to}")))
  })
  .await
}

/// Put a resolved note into a board, as the board picker shown after
/// collecting does.
pub async fn place(api: &Api, board: &str, note: &NoteRef) -> Result<()> {
  let body = json!({"target_board_id": board, "notes_id": note.id});
  api.post(MOVE, &body).await?;
  Ok(())
}

async fn place_ref(api: &Api, board: &str, arg: &str) -> Result<NoteRef> {
  let note = refs::note_ref(api, arg).await?;
  api.write_pause().await;
  place(api, board, &note).await?;
  Ok(note)
}

/// One board as the board page loads it.
async fn info(api: &Api, id: &str) -> Result<Value> {
  let path = format!("{BOARD}/{id}");
  let data = api
    .get(&path, &[("image_formats", "jpg,webp,avif")])
    .await?;
  match data.str("name") {
    Some(_) => Ok(data),
    None => Err(Error::not_found(format!("no board {id}"))),
  }
}

/// The limits of the web's board form.
fn check(spec: &FolderSpec) -> Result<()> {
  if spec
    .name
    .as_ref()
    .is_some_and(|n| n.chars().count() > NAME_MAX)
  {
    return Err(Error::input(format!(
      "a board name has at most {NAME_MAX} characters"
    )));
  }
  if spec
    .description
    .as_ref()
    .is_some_and(|d| d.chars().count() > DESC_MAX)
  {
    return Err(Error::input(format!(
      "a board description has at most {DESC_MAX} characters"
    )));
  }
  Ok(())
}

/// `privacy` as the web sends it: 1 private, 0 public.
fn privacy(private: bool) -> i64 {
  i64::from(private)
}

fn board(v: &Value) -> Option<Collection> {
  let id = v.first_str(&["id", "board_id"])?;
  let cover = v
    .list("images")
    .first()
    .and_then(|i| i.as_str().map(str::to_owned).or_else(|| i.str("url")));
  let mut collection = Collection {
    url: Some(board_url(&id)),
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

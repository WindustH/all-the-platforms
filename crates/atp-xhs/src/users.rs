//! Accounts: the logged-in user, profiles, their note lists, and searching
//! your own collected or liked notes.

use atp_core::{Error, Page, PageReq, Post, Result, User, Value, ValueExt};

use crate::api::Api;
use crate::refs::{self, SOURCE_FEED};
use crate::{parse, sign};

pub const ME: &str = "/api/sns/web/v2/user/me";
/// Search inside the collected / liked tabs of your own profile.
const SAVED_SEARCH: &str = "/api/sns/v1/note/user/page/search/web";

/// The logged-in account; a guest session counts as not logged in.
pub async fn whoami(api: &Api) -> Result<User> {
  api.require_login()?;
  let me = api.get(ME, &[]).await?;
  if me.bool("guest") == Some(true) {
    return Err(Error::auth("not logged in (guest session)").with_hint("run `atp xhs login`"));
  }
  Ok(parse::profile(&me, None))
}

/// The logged-in account's id, looked up once per command.
pub async fn my_id(api: &Api) -> Result<String> {
  if let Some(id) = api.me.get() {
    return Ok(id.clone());
  }
  let id = whoami(api).await?.id;
  Ok(api.me.get_or_init(|| id).clone())
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  api.require_login()?;
  let id = refs::user_ref(api, arg).await?;
  let data = api
    .get("/api/sns/web/v1/user/otherinfo", &[("target_user_id", &id)])
    .await?;
  Ok(parse::profile(&data, Some(&id)))
}

pub async fn user_posts(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  api.require_login()?;
  let id = refs::user_ref(api, arg).await?;
  let num = page.size_within(30).to_string();
  let cursor = page.cursor.clone().unwrap_or_default();
  let params = [
    ("num", num.as_str()),
    ("cursor", cursor.as_str()),
    ("user_id", id.as_str()),
    ("image_scenes", "FD_WM_WEBP"),
  ];
  Ok(note_page(
    api,
    &api.get("/api/sns/web/v1/user_posted", &params).await?,
  ))
}

/// Notes a user collected (default: the logged-in account).
pub async fn favorites(api: &Api, user: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  saved(api, "/api/sns/web/v2/note/collect/page", user, page).await
}

/// Notes a user liked (default: the logged-in account).
pub async fn likes(api: &Api, user: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  saved(api, "/api/sns/web/v1/note/like/page", user, page).await
}

async fn saved(api: &Api, path: &str, user: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  api.require_login()?;
  let id = match user {
    Some(arg) => refs::user_ref(api, arg).await?,
    None => whoami(api).await?.id,
  };
  let num = page.size_within(30).to_string();
  let cursor = page.cursor.clone().unwrap_or_default();
  let params = [
    ("user_id", id.as_str()),
    ("cursor", cursor.as_str()),
    ("num", num.as_str()),
  ];
  Ok(note_page(api, &api.get(path, &params).await?))
}

/// Your collected (or with `likes`, liked) notes that match `keyword`.
pub async fn search_saved(
  api: &Api,
  keyword: &str,
  likes: bool,
  req: &PageReq,
) -> Result<Page<Post>> {
  api.require_login()?;
  let me = whoami(api).await?.id;
  let tab = if likes { "like" } else { "collect" };
  let cursor = req.cursor.clone().unwrap_or_default();
  let size = req.size_within(30).to_string();
  let search_id = sign::random::search_id(sign::now_ms());
  let params = [
    ("user_id", me.as_str()),
    ("view_user_id", me.as_str()),
    ("type", tab),
    ("cursor", cursor.as_str()),
    ("page_size", size.as_str()),
    ("keyword", keyword.trim()),
    ("search_id", search_id.as_str()),
  ];
  let data = api.get(SAVED_SEARCH, &params).await?;
  Ok(note_page(api, data.at(tab)))
}

/// `{notes, cursor, has_more}` pages, remembering the note tokens.
pub(crate) fn note_page(api: &Api, data: &Value) -> Page<Post> {
  let notes = data.list("notes");
  let tokens: Vec<(String, String)> = notes
    .iter()
    .filter_map(|n| Some((n.str("note_id")?, n.str("xsec_token")?)))
    .collect();
  refs::remember(
    &api.ctx,
    tokens
      .iter()
      .map(|(id, t)| (id.as_str(), t.as_str(), SOURCE_FEED)),
  );
  let items = notes
    .iter()
    .filter_map(|n| parse::note(n, SOURCE_FEED))
    .collect();
  let next = data
    .str("cursor")
    .filter(|_| data.bool("has_more") == Some(true));
  Page::new(items, next)
}

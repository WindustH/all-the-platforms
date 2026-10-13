//! Accounts: the logged-in user, profiles, their note lists, the accounts
//! you follow, and searching your own collected or liked notes.

use social_core::{Error, Page, PageReq, Post, Result, User, Value, ValueExt};

use crate::api::Client;
use crate::refs::{self, SOURCE_FEED, user_url};
use crate::{parse, sign};

pub const ME: &str = "/api/sns/web/v2/user/me";
/// The message page's contact picker: everyone the logged-in account follows.
const FOLLOWING: &str = "/api/im/web/users/following/all";
/// Search inside the collected / liked tabs of your own profile.
const SAVED_SEARCH: &str = "/api/sns/v1/note/user/page/search/web";

/// The logged-in account; a guest session counts as not logged in.
pub async fn whoami(c: &Client) -> Result<User> {
  c.require_login()?;
  let me = c.get(ME, &[]).await?;
  if me.bool("guest") == Some(true) {
    return Err(Error::auth("not logged in (guest session)").with_hint("run `social xhs login`"));
  }
  Ok(parse::profile(&me, None))
}

pub async fn user(c: &Client, arg: &str) -> Result<User> {
  c.require_login()?;
  let id = refs::user_ref(c, arg).await?;
  let data = c
    .get("/api/sns/web/v1/user/otherinfo", &[("target_user_id", &id)])
    .await?;
  Ok(parse::profile(&data, Some(&id)))
}

pub async fn user_posts(c: &Client, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  c.require_login()?;
  let id = refs::user_ref(c, arg).await?;
  let num = page.size_within(30).to_string();
  let cursor = page.cursor.clone().unwrap_or_default();
  let params = [
    ("num", num.as_str()),
    ("cursor", cursor.as_str()),
    ("user_id", id.as_str()),
    ("image_scenes", "FD_WM_WEBP"),
  ];
  Ok(note_page(
    c,
    &c.get("/api/sns/web/v1/user_posted", &params).await?,
  ))
}

/// Notes a user collected (default: the logged-in account).
pub async fn favorites(c: &Client, user: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  saved(c, "/api/sns/web/v2/note/collect/page", user, page).await
}

/// Notes a user liked (default: the logged-in account).
pub async fn likes(c: &Client, user: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  saved(c, "/api/sns/web/v1/note/like/page", user, page).await
}

async fn saved(c: &Client, path: &str, user: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  c.require_login()?;
  let id = match user {
    Some(arg) => refs::user_ref(c, arg).await?,
    None => whoami(c).await?.id,
  };
  let num = page.size_within(30).to_string();
  let cursor = page.cursor.clone().unwrap_or_default();
  let params = [
    ("user_id", id.as_str()),
    ("cursor", cursor.as_str()),
    ("num", num.as_str()),
  ];
  Ok(note_page(c, &c.get(path, &params).await?))
}

/// Accounts the logged-in user follows. Xiaohongshu shows other accounts'
/// follow lists only as counts.
pub async fn following(c: &Client, user: &str, req: &PageReq) -> Result<Page<User>> {
  c.require_login()?;
  let me = whoami(c).await?.id;
  let target = refs::user_ref(c, user).await?;
  if target != me {
    return Err(Error::unsupported(
      "following of other accounts (Xiaohongshu shows only the count)",
    ));
  }
  let page = req.number_or(1);
  let (page_s, size) = (page.to_string(), req.size_within(200).to_string());
  let data = c
    .get(
      FOLLOWING,
      &[("page", page_s.as_str()), ("size", size.as_str())],
    )
    .await?;
  let rows = data.list("follow_user_d_t_o_list");
  let items: Vec<User> = rows
    .iter()
    .filter_map(|u| {
      let id = u.str("user_id")?;
      Some(User {
        url: Some(user_url(&id)),
        name: u.str("nick_name").unwrap_or_default(),
        avatar: u.str("avatar"),
        verified: u.i64("verify_type").is_some_and(|t| t > 0),
        followed: Some(true),
        id,
        ..User::default()
      })
    })
    .collect();
  let next = (!items.is_empty()).then(|| (page + 1).to_string());
  Ok(Page::new(items, next))
}

/// Your collected (or with `likes`, liked) notes that match `keyword`.
pub async fn search_saved(
  c: &Client,
  keyword: &str,
  likes: bool,
  req: &PageReq,
) -> Result<Page<Post>> {
  c.require_login()?;
  let me = whoami(c).await?.id;
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
  let data = c.get(SAVED_SEARCH, &params).await?;
  Ok(note_page(c, data.at(tab)))
}

/// `{notes, cursor, has_more}` pages, remembering the note tokens.
pub(crate) fn note_page(c: &Client, data: &Value) -> Page<Post> {
  let notes = data.list("notes");
  let tokens: Vec<(String, String)> = notes
    .iter()
    .filter_map(|n| Some((n.str("note_id")?, n.str("xsec_token")?)))
    .collect();
  refs::remember(
    &c.ctx,
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

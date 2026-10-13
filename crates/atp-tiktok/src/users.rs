//! Users: the logged-in account, profiles, their videos, liked and saved
//! videos, followers and following, playlists and favorites collections.

use atp_core::{Collection, Error, Page, PageReq, Post, Result, User, Value, ValueExt};

use crate::api::{Api, WWW, node, status_error};
use crate::parse;
use crate::refs::{self, UserRef};
use crate::videos::{Params, item_page, next_cursor};

/// The logged-in account, as the app context of any page names it.
pub async fn whoami(api: &Api) -> Result<User> {
  api.require_login()?;
  api.page(&format!("{WWW}/foryou")).await?;
  let me = api.app().await?.user.ok_or_else(|| {
    Error::auth("TikTok did not recognize the session").with_hint(api.ctx.login_hint())
  })?;
  let handle = me
    .first_str(&["uniqueId", "unique_id"])
    .ok_or_else(|| Error::upstream("the session names no account"))?;
  match profile(api, &handle).await {
    Ok(user) => Ok(user),
    Err(_) => Ok(parse::user_brief(&me)),
  }
}

/// The handle of the logged-in account (session extra after the first lookup).
async fn own_handle(api: &Api) -> Result<String> {
  if let Some(h) = api.ctx.extra("handle") {
    return Ok(h);
  }
  let me = whoami(api).await?;
  let handle = me.handle.unwrap_or_default();
  api.ctx.set_extra("handle", &handle);
  Ok(handle)
}

/// A profile page's `userInfo`.
async fn profile_info(api: &Api, handle: &str) -> Result<Value> {
  let scope = api.page(&refs::user_url(handle)).await?;
  let detail = node(&scope, "webapp.user-detail");
  let code = detail.i64("statusCode").unwrap_or(0);
  if code != 0 {
    let msg = detail.str("statusMsg").unwrap_or_default();
    return Err(status_error(&format!("@{handle}"), code, &msg));
  }
  let info = detail.at("userInfo");
  if info.str("user.uniqueId").is_none() {
    // A stripped page; the API has the profile.
    let v = api
      .get(
        "/api/user/detail/",
        &[("uniqueId", handle.to_owned()), ("secUid", String::new())],
      )
      .await?;
    return Ok(v.at("userInfo").clone());
  }
  Ok(info.clone())
}

async fn profile(api: &Api, handle: &str) -> Result<User> {
  Ok(parse::user(&profile_info(api, handle).await?))
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  match refs::user_ref(api, arg).await? {
    UserRef::Handle(h) => profile(api, &h).await,
    UserRef::SecUid(s) => {
      let v = api
        .get(
          "/api/user/detail/",
          &[("secUid", s.clone()), ("uniqueId", String::new())],
        )
        .await?;
      if v.str("userInfo.user.id").is_none() {
        return Err(Error::not_found(format!("no user {s}")));
      }
      Ok(parse::user(v.at("userInfo")))
    }
  }
}

/// The secUid and numeric id of a user (`None` = the logged-in account).
pub async fn ids(api: &Api, arg: Option<&str>) -> Result<(String, String)> {
  let user_ref = match arg {
    Some(a) => refs::user_ref(api, a).await?,
    None => UserRef::Handle(own_handle(api).await?),
  };
  let u = match user_ref {
    UserRef::SecUid(s) => {
      let v = api
        .get(
          "/api/user/detail/",
          &[("secUid", s), ("uniqueId", String::new())],
        )
        .await?;
      parse::user(v.at("userInfo"))
    }
    UserRef::Handle(h) => profile(api, &h).await?,
  };
  let sec = u
    .extra
    .get("sec_uid")
    .and_then(|v| v.as_str())
    .map(str::to_owned)
    .ok_or_else(|| Error::upstream("the profile carries no secUid"))?;
  Ok((sec, u.id))
}

pub async fn posts(api: &Api, arg: &str, req: &PageReq) -> Result<Page<Post>> {
  let (sec, _) = ids(api, Some(arg)).await?;
  let params: Params = vec![
    ("secUid", sec),
    ("coverFormat", "2".into()),
    ("post_item_list_request_type", "0".into()),
  ];
  item_page(api, "/api/post/item_list/", params, req, 35).await
}

/// Videos a user liked (public only when the user allows it).
pub async fn likes(api: &Api, arg: Option<&str>, req: &PageReq) -> Result<Page<Post>> {
  let (sec, _) = ids(api, arg).await?;
  item_page(
    api,
    "/api/favorite/item_list/",
    vec![("secUid", sec)],
    req,
    30,
  )
  .await
}

/// Saved videos, or those of one favorites collection.
pub async fn favorites(
  api: &Api,
  arg: Option<&str>,
  folder: Option<&str>,
  req: &PageReq,
) -> Result<Page<Post>> {
  if let Some(folder) = folder {
    let id = refs::collection_id(folder)?;
    return item_page(
      api,
      "/api/collection/item_list/",
      vec![("collectionId", id), ("sourceType", "113".into())],
      req,
      30,
    )
    .await;
  }
  let (sec, _) = ids(api, arg).await?;
  item_page(
    api,
    "/api/user/collect/item_list/",
    vec![("secUid", sec)],
    req,
    30,
  )
  .await
}

/// Favorites collections (folders of saved videos).
pub async fn collections(api: &Api, arg: Option<&str>, req: &PageReq) -> Result<Page<Collection>> {
  let (sec, _) = ids(api, arg).await?;
  folders(api, "/api/user/collection_list/", sec, req, "collection").await
}

/// Playlists (mixes) a creator made of their videos.
pub async fn playlists(api: &Api, arg: Option<&str>, req: &PageReq) -> Result<Page<Collection>> {
  let (sec, _) = ids(api, arg).await?;
  folders(api, "/api/user/playlist/", sec, req, "playlist").await
}

async fn folders(
  api: &Api,
  path: &str,
  sec: String,
  req: &PageReq,
  kind: &str,
) -> Result<Page<Collection>> {
  let params = vec![
    ("secUid", sec),
    ("count", req.size_within(20).to_string()),
    ("cursor", req.cursor.clone().unwrap_or_else(|| "0".into())),
  ];
  let v = api.get(path, &params).await?;
  let items = ["playList", "collectionList", "collection_list", "mixList"]
    .iter()
    .flat_map(|k| v.list(k))
    .filter_map(|f| parse::folder(f, kind))
    .collect();
  Ok(Page::new(items, next_cursor(&v)))
}

/// Followers (`scene` 67) or followed accounts (21); paged by `minCursor`.
async fn relations(api: &Api, arg: &str, scene: &str, req: &PageReq) -> Result<Page<User>> {
  let (sec, _) = ids(api, Some(arg)).await?;
  let params = vec![
    ("secUid", sec),
    ("scene", scene.to_owned()),
    ("count", req.size_within(30).to_string()),
    ("maxCursor", "0".into()),
    (
      "minCursor",
      req.cursor.clone().unwrap_or_else(|| "0".into()),
    ),
  ];
  let v = api.get("/api/user/list/", &params).await?;
  let items = v.list("userList").iter().map(parse::listed_user).collect();
  let more = v.bool("hasMore").unwrap_or(false);
  Ok(Page::new(items, more.then(|| v.str("minCursor")).flatten()))
}

pub async fn followers(api: &Api, arg: &str, req: &PageReq) -> Result<Page<User>> {
  relations(api, arg, "67", req).await
}

pub async fn following(api: &Api, arg: &str, req: &PageReq) -> Result<Page<User>> {
  relations(api, arg, "21", req).await
}

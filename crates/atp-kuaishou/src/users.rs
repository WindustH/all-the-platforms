//! The account, profiles and users' posts; the account's own follow lists,
//! likes and favorites (Kuaishou's web shows those to the owner only).

use atp_core::{Error, ErrorCode, Page, PageReq, Post, Result, User, Value, ValueExt, json};

use crate::api::{Api, LOGIN_HINT, WWW};
use crate::{parse, refs};

/// Verbatim from the web (the server checks the text).
const USER_INFO: &str = "query userInfoQuery {\n  userInfo {\n    id\n    name\n    avatar\n    eid\n    userId\n    __typename\n  }\n}\n";

const VISION_PROFILE_REDUCED: &str = "query visionProfileReduced($userId: String) {\n  visionProfileReduced(userId: $userId) {\n    result\n    hostName\n    userProfile {\n      profile {\n        gender\n        user_name\n        user_id\n        headurl\n        user_text\n        user_profile_bg_url\n        __typename\n      }\n      isFollowing\n      isUserIsolated\n      livingInfo\n      __typename\n    }\n    __typename\n  }\n}\n";

/// The account: `profile/get` (with counters), else the `userInfo` query.
pub async fn whoami(api: &Api) -> Result<User> {
  api.require_login()?;
  match api.www_get("/rest/v/profile/get").send().await {
    Ok(v) => {
      if let Some(user) = parse::me(&v) {
        return Ok(user);
      }
    }
    Err(e) if e.code == ErrorCode::NotAuthenticated => return Err(e),
    Err(e) => tracing::debug!("profile/get: {e}"),
  }
  let data = api
    .graphql(
      "userInfoQuery",
      json!({}),
      USER_INFO,
      &format!("{WWW}/myFollow"),
    )
    .await?;
  parse::user_info(data.at("userInfo"))
    .ok_or_else(|| Error::auth("Kuaishou does not know this session").with_hint(LOGIN_HINT))
}

fn is_me(arg: &str) -> bool {
  matches!(arg.trim(), "" | "me" | "@me")
}

/// The eid of a user reference, `me` included.
pub async fn eid(api: &Api, arg: &str) -> Result<String> {
  if is_me(arg) {
    return my_eid(api).await;
  }
  refs::user(api, arg).await
}

pub async fn profile(api: &Api, arg: &str) -> Result<User> {
  if is_me(arg) {
    return whoami(api).await;
  }
  let eid = refs::user(api, arg).await?;
  let data = api
    .graphql(
      "visionProfileReduced",
      json!({ "userId": eid }),
      VISION_PROFILE_REDUCED,
      &refs::profile_url(&eid),
    )
    .await?;
  let v = crate::api::check(data.at("visionProfileReduced").clone())?;
  parse::profile(&v).ok_or_else(|| Error::not_found(format!("no Kuaishou user {eid}")))
}

pub async fn posts(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let eid = eid(api, arg).await?;
  let v = api
    .www("/rest/v/profile/feed")
    .json(json!({ "user_id": eid, "pcursor": cursor(page), "page": "profile" }))
    .referer(&refs::profile_url(&eid))
    .header("profile_referer", "")
    .send()
    .await?;
  Ok(posts_page(&v))
}

/// The account's eid (asked once per run).
async fn my_eid(api: &Api) -> Result<String> {
  if let Some(me) = api.me.get() {
    return Ok(me.clone());
  }
  let me = whoami(api).await?.id;
  Ok(api.me.get_or_init(|| me).clone())
}

/// The account's eid, failing for other users' private lists.
async fn own(api: &Api, user: Option<&str>, what: &str) -> Result<String> {
  api.require_login()?;
  let me = my_eid(api).await?;
  match user {
    Some(arg) if !is_me(arg) && refs::user(api, arg).await? != me => {
      Err(Error::unsupported(format!("{what} of other users")))
    }
    _ => Ok(me),
  }
}

pub async fn followers(api: &Api, user: &str, page: &PageReq) -> Result<Page<User>> {
  let me = own(api, Some(user), "followers").await?;
  relation(api, &me, 2, page).await
}

pub async fn following(api: &Api, user: &str, page: &PageReq) -> Result<Page<User>> {
  let me = own(api, Some(user), "following").await?;
  relation(api, &me, 1, page).await
}

/// `ftype` 1: who the account follows; 2: its followers.
async fn relation(api: &Api, me: &str, ftype: u8, page: &PageReq) -> Result<Page<User>> {
  let v = api
    .www("/rest/v/relation/fol")
    .json(json!({ "pcursor": cursor(page), "ftype": ftype }))
    .referer(&refs::profile_url(me))
    .send()
    .await?;
  let users: Vec<User> = ["authors", "fols", "users"]
    .iter()
    .flat_map(|k| v.list(k))
    .filter_map(parse::listed_user)
    .collect();
  let next = parse::more(v.str("pcursor")).filter(|_| !users.is_empty());
  Ok(Page::new(users, next))
}

pub async fn likes(api: &Api, user: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let me = own(api, user, "likes").await?;
  let v = api
    .www("/rest/v/feed/liked")
    .json(json!({ "pcursor": cursor(page), "page": "profile" }))
    .referer(&refs::profile_url(&me))
    .send()
    .await?;
  Ok(posts_page(&v))
}

pub async fn favorites(
  api: &Api,
  user: Option<&str>,
  folder: Option<&str>,
  page: &PageReq,
) -> Result<Page<Post>> {
  if folder.is_some() {
    return Err(Error::unsupported("favorites --folder"));
  }
  let me = own(api, user, "favorites").await?;
  let v = api
    .www("/rest/v/collect/list")
    .json(json!({ "userId": me, "pcursor": cursor(page), "page": "collect" }))
    .referer(&refs::profile_url(&me))
    .send()
    .await?;
  Ok(posts_page(&v))
}

fn cursor(page: &PageReq) -> String {
  page.cursor.clone().unwrap_or_default()
}

/// `{feeds, pcursor}` lists of posts.
fn posts_page(v: &Value) -> Page<Post> {
  let posts: Vec<Post> = v.list("feeds").iter().filter_map(parse::post).collect();
  let next = parse::more(v.str("pcursor")).filter(|_| !posts.is_empty());
  Page::new(posts, next)
}

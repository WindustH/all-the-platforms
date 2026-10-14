//! Accounts and shops. `whoami` asks the H5 site who is logged in
//! (`apollo/v3/user/me`, as the pages do), else checks the 多多进宝
//! credentials with `pdd.ddk.goods.pid.query`. `user MALL` reads the
//! shop card the goods page shows (`turing/mall/<id>/simpleinfo`) with a
//! login, else the shop's page (`mall_page.html`). `user-posts MALL` lists
//! a shop's goods (`turing/mall/query_cat_goods`, the app's call, with a
//! login and `anti_content`).

use atp_core::{Error, ErrorCode, Page, PageReq, Post, Result, User, ValueExt};

use crate::api::{Api, WEB, image_url, list_id, mall_url};
use crate::{ddk, parse, refs};

const ME: &str = "/api/apollo/v3/user/me";

pub async fn whoami(api: &Api) -> Result<User> {
  if api.has_session() {
    return profile(api).await;
  }
  if api.ddk.has(&api.ctx) {
    return app(api).await;
  }
  Err(Error::auth("not logged in to Pinduoduo").with_hint(api.either_hint()))
}

/// The H5 account; when it cannot be read for reasons other than a login
/// wall, the session's user id stands in (`extra.checked` false).
async fn profile(api: &Api) -> Result<User> {
  let uid = api.uid();
  let from = format!("{WEB}/personal.html");
  match api.get_plain(ME, &[], &from).await {
    Ok(v) => {
      let v = if v.get("result").is_some() {
        v.at("result").clone()
      } else {
        v
      };
      Ok(User {
        id: v.str("uid").unwrap_or_else(|| uid.clone()),
        name: v
          .first_str(&["nickname", "nick_name", "name"])
          .unwrap_or_else(|| uid.clone()),
        avatar: v.str("avatar").map(|u| image_url(&u)),
        url: Some(from),
        raw: Some(v),
        ..User::default()
      })
    }
    Err(e)
      if matches!(
        e.code,
        ErrorCode::NotAuthenticated | ErrorCode::VerificationRequired
      ) =>
    {
      Err(e)
    }
    Err(e) if e.code == ErrorCode::RateLimited => Err(
      Error::auth(format!(
        "Pinduoduo did not accept the session: {}",
        e.message
      ))
      .with_hint(api.ctx.login_hint()),
    ),
    Err(_) => {
      let mut user = User {
        name: uid.clone(),
        id: uid,
        url: Some(from),
        ..User::default()
      };
      user.extra.insert("checked".into(), false.into());
      Ok(user)
    }
  }
}

/// Keep DDK credentials and check them against the API.
pub async fn login_token(api: &Api, token: &str) -> Result<()> {
  ddk::Ddk::save(&api.ctx, token)?;
  app(api).await.map(|_| ())
}

/// The DDK app behind the credentials, with its configured pid: what
/// `login --token` checks, also beside an H5 session.
pub async fn app(api: &Api) -> Result<User> {
  let creds = api
    .ddk
    .creds(&api.ctx)
    .ok_or_else(|| Error::auth("no 多多进宝 credentials").with_hint(ddk::LOGIN_HINT))?;
  let v = api
    .ddk
    .call(
      &api.ctx,
      "pdd.ddk.goods.pid.query",
      &[("page", "1".into()), ("page_size", "50".into())],
    )
    .await?;
  let pids = v.list("p_id_query_response.p_id_list");
  let mine = pids
    .iter()
    .find(|p| p.str("p_id").as_deref() == Some(creds.pid.as_str()));
  let mut user = User {
    id: creds.client_id.clone(),
    name: mine
      .and_then(|p| p.str("pid_name"))
      .unwrap_or_else(|| format!("多多进宝 {}", creds.client_id)),
    raw: Some(v.clone()),
    ..User::default()
  };
  user.extra.insert("pid".into(), creds.pid.clone().into());
  user.extra.insert("pid_found".into(), mine.is_some().into());
  user.stats.posts = v.u64("p_id_query_response.total_count");
  Ok(user)
}

/// A shop: its card (logged in), else its page.
pub async fn user(api: &Api, user: &str) -> Result<User> {
  let id = refs::mall(user)?;
  if api.has_session() {
    let path = format!("/api/turing/mall/{id}/simpleinfo");
    let v = api.get_plain(&path, &[], &mall_url(&id)).await?;
    if let Some(found) = parse::mall(&v, &id) {
      return Ok(found);
    }
  }
  let store = api.store(&mall_url(&id)).await?;
  parse::mall(&store, &id).ok_or_else(|| {
    Error::not_found(format!("no shop data for {id}")).with_hint(if api.has_session() {
      "the shop may be closed".to_owned()
    } else {
      format!(
        "shops show their data after a login; {}",
        api.ctx.login_hint()
      )
    })
  })
}

/// A shop's goods, 20 a page, best sellers first; cursors are
/// `<page>:<list_id>`.
pub async fn goods(api: &Api, user: &str, page: &PageReq) -> Result<Page<Post>> {
  api.require_session()?;
  let id = refs::mall(user)?;
  let (number, list) = page
    .cursor
    .as_deref()
    .and_then(|c| c.split_once(':'))
    .map(|(n, l)| (n.parse::<u64>().unwrap_or(1), l.to_owned()))
    .unwrap_or_else(|| (1, list_id()));
  let v = api
    .get(
      "/api/turing/mall/query_cat_goods",
      &[
        ("mall_id", id.clone()),
        ("page_no", number.to_string()),
        ("page_size", "20".into()),
        ("sort_type", "_sales".into()),
        ("category_id", "0".into()),
        ("type", "0".into()),
        ("list_id", list.clone()),
      ],
      &mall_url(&id),
    )
    .await?;
  let items = parse::goods_list(&v);
  let next = (items.len() >= 20).then(|| format!("{}:{list}", number + 1));
  Ok(Page::new(items, next))
}

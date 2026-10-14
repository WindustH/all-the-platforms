//! Accounts and shops: `whoami` asks passport whether the session is
//! logged in; `user SHOP` reads a shop's home page on `mall.jd.com`.

use atp_core::{Error, Result, User, Value, ValueExt};

use crate::api::{Api, WWW, shop_url};
use crate::{parse, refs};

const SESSION_CHECK: &str =
  "https://passport.jd.com/loginservice.aspx?method=Login&callback=jsonpLogin";

pub async fn whoami(api: &Api) -> Result<User> {
  api.require_login()?;
  let text = api
    .ctx
    .http
    .get(SESSION_CHECK)
    .header("accept", "*/*")
    .header("referer", format!("{WWW}/"))
    .header("sec-fetch-dest", "script")
    .send()
    .await?
    .check()?
    .text();
  let json = text
    .split_once('(')
    .and_then(|(_, rest)| rest.rsplit_once(')'))
    .map_or(text.as_str(), |(inner, _)| inner);
  let v: Value = serde_json::from_str(json)?;
  let identity = v.at("Identity");
  if identity.bool("IsAuthenticated") != Some(true) {
    return Err(Error::auth("the JD session has expired").with_hint(api.ctx.login_hint()));
  }
  let pin = api.pin().unwrap_or_default();
  Ok(User {
    name: identity
      .first_str(&["Unick", "Name"])
      .unwrap_or_else(|| pin.clone()),
    handle: Some(pin.clone()).filter(|p| !p.is_empty()),
    url: Some("https://home.jd.com/".into()),
    id: pin,
    raw: Some(v.clone()),
    ..User::default()
  })
}

/// A shop: its name, ids and main category.
pub async fn user(api: &Api, user: &str) -> Result<User> {
  let id = refs::shop(user)?;
  let html = api.page(&shop_url(&id)).await?;
  parse::shop_page(&html, &id).ok_or_else(|| Error::not_found(format!("JD has no shop {id}")))
}

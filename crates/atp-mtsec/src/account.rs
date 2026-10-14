//! Who a Dianping session belongs to: `accountapi.dianping.com/user/v1/info`
//! answers the session's account, or a login error (401, `请重新登录`)
//! without one. The cookie `ua` holds the nickname the site shows.

use atp_core::{Ctx, Result, User, Value, ValueExt};

use crate::guard::{self, Call, DIANPING};
use crate::shops::{MOBILE, WWW};

const INFO: &str = "https://accountapi.dianping.com/user/v1/info";

/// The account of the session's Dianping login.
pub async fn dianping(ctx: &Ctx) -> Result<User> {
  let call = Call::xhr(INFO, format!("{MOBILE}/"));
  let v = guard::json(ctx, &DIANPING, call).await?;
  Ok(user(ctx, v))
}

fn user(ctx: &Ctx, v: Value) -> User {
  let data = ["data.user", "data", "user"]
    .iter()
    .map(|k| v.at(k))
    .find(|d| d.is_object())
    .unwrap_or(&v)
    .clone();
  let id = data
    .first_str(&["userId", "userid", "id", "mtUserId"])
    .unwrap_or_default();
  let nickname = ctx.http.cookie("ua").map(|n| decode(&n));
  let name = data
    .first_str(&["nickName", "userNickName", "nickname", "userName", "name"])
    .or(nickname)
    .unwrap_or_else(|| id.clone());
  User {
    url: (!id.is_empty()).then(|| format!("{WWW}/member/{id}")),
    avatar: data.first_str(&["avatar", "userFace", "face", "avatarUrl"]),
    id,
    name,
    raw: Some(v),
    ..User::default()
  }
}

/// A percent-encoded cookie value as text.
fn decode(value: &str) -> String {
  url::form_urlencoded::parse(format!("v={value}").as_bytes())
    .next()
    .map(|(_, v)| v.into_owned())
    .unwrap_or_else(|| value.to_owned())
}

//! `whoami`: the 美团联盟 media account when its credentials are set (a
//! one-deal `query_coupon` proves them), else the session's Dianping
//! login.

use atp_core::{Error, Result, User, json};
use atp_mtsec::account;

use crate::api::Api;

pub async fn whoami(api: &Api) -> Result<User> {
  if let Some(creds) = api.cps.creds(&api.ctx) {
    let probe = json!({ "platform": 1, "pageNo": 1, "pageSize": 1 });
    api.cps.call(&api.ctx, "query_coupon", &probe).await?;
    let mut user = User {
      name: format!("美团联盟 app {}", creds.key),
      id: creds.key,
      ..User::default()
    };
    user.extra.insert("route".into(), "cps".into());
    return Ok(user);
  }
  if api.has_dianping() {
    return account::dianping(&api.ctx).await;
  }
  Err(Error::auth("not logged in to Meituan").with_hint(api.either_hint()))
}

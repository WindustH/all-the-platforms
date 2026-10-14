//! `whoami`: Amap keys have no account behind them, so this checks the key
//! with a cheap IP lookup and describes it.

use atp_core::credentials;
use atp_core::{Result, User, ValueExt};

use crate::api::Api;

pub async fn whoami(api: &Api) -> Result<User> {
  let creds = api.require()?;
  let v = api.get("/v3/ip", &[]).await?;
  let location = [v.str("province"), v.str("city")]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ");
  let mut user = User {
    id: credentials::mask(&creds.key),
    name: "Amap Web Service key".into(),
    url: Some("https://console.amap.com/dev/key/app".into()),
    location: (!location.is_empty()).then_some(location),
    ..User::default()
  };
  user
    .extra
    .insert("signed".into(), creds.secret.is_some().into());
  Ok(user)
}

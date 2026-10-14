//! `whoami`: a key has no account behind it, so this checks the key with
//! a cheap IP lookup and describes it.

use atp_core::{Result, User, ValueExt, credentials};

use crate::api::Api;

pub async fn whoami(api: &Api) -> Result<User> {
  let creds = api.require()?;
  let v = api.get("/ws/location/v1/ip", &[]).await?;
  let location = ["province", "city"]
    .iter()
    .filter_map(|k| v.str(&format!("result.ad_info.{k}")))
    .collect::<Vec<_>>()
    .join(" ");
  let mut user = User {
    id: credentials::mask(&creds.key),
    name: "Tencent Map key".into(),
    url: Some("https://lbs.qq.com/dev/console/application/mine".into()),
    location: (!location.is_empty()).then_some(location),
    ..User::default()
  };
  user
    .extra
    .insert("signed".into(), creds.sk.is_some().into());
  Ok(user)
}

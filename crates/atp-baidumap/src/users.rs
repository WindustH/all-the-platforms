//! `whoami`: an AK has no account behind it, so this checks the AK with a
//! cheap IP lookup and describes it.

use atp_core::{Result, User, Value, ValueExt, credentials};

use crate::api::Api;

pub async fn whoami(api: &Api) -> Result<User> {
  let creds = api.require()?;
  let v: Value = api.get("/location/ip", &[]).await?;
  let mut user = User {
    id: credentials::mask(&creds.ak),
    name: "Baidu Map AK".into(),
    url: Some("https://lbsyun.baidu.com/apiconsole/key".into()),
    location: v.str("content.address"),
    ..User::default()
  };
  user
    .extra
    .insert("signed".into(), creds.sk.is_some().into());
  Ok(user)
}

//! The app itself and staff members.

use atp_core::{Result, User, Value, ValueExt, json};

use crate::api::{Api, OapiBody};
use crate::parse;

/// The app and its robot code (DingTalk has no "who am I" for robots).
pub async fn whoami(api: &Api) -> Result<User> {
  let creds = api.require()?;
  api.token().await?;
  Ok(User {
    id: creds.robot_code.clone(),
    name: format!("DingTalk app {}", creds.app_key),
    extra: [
      ("app_key".to_owned(), Value::from(creds.app_key)),
      ("robot_code".to_owned(), Value::from(creds.robot_code)),
    ]
    .into_iter()
    .collect(),
    ..User::default()
  })
}

/// One staff member (`topapi/v2/user/get`).
pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let id = arg.trim().trim_start_matches("user:");
  let body = OapiBody::Json(json!({ "userid": id, "language": "zh_CN" }));
  let v = api.oapi("/topapi/v2/user/get", &[], body).await?;
  Ok(parse::staff(id, v.at("result")))
}

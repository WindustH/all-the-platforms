//! Groups and rooms the bot is in.

use atp_core::{Action, Error, Result, json};

use crate::api::Api;
use crate::refs::{self, ChatRef};

pub async fn leave(api: &Api, arg: &str) -> Result<Action> {
  let (kind, id) = match refs::chat(arg)? {
    ChatRef::Group(id) => ("group", id),
    ChatRef::Room(id) => ("room", id),
    _ => return Err(Error::input("leave a group (C…) or room (R…)")),
  };
  api
    .post(&format!("/v2/bot/{kind}/{id}/leave"), &json!({}))
    .await?;
  Ok(Action::done("leave", id))
}

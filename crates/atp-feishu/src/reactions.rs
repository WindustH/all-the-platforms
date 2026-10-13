//! Reactions on messages, by Feishu's reaction names.

use atp_core::{Action, Error, Result, ValueExt, json};

use crate::api::Api;
use crate::refs;

/// Feishu names reactions (`THUMBSUP`, `SMILE` …); a few emoji are
/// translated, any other value is passed on as the name.
fn emoji_type(emoji: &str) -> String {
  match emoji.trim() {
    "👍" | "+1" | "like" => "THUMBSUP".into(),
    "👌" | "ok" => "OK".into(),
    "😄" | "😀" | "🙂" | "smile" => "SMILE".into(),
    other => other.to_ascii_uppercase(),
  }
}

pub async fn react(api: &Api, arg: &str, emoji: &str, undo: bool) -> Result<Action> {
  let id = refs::message(arg)?;
  let kind = emoji_type(emoji);
  if undo {
    unreact(api, &id, &kind).await?;
    return Ok(Action::done("unreact", &id).with_message(kind));
  }
  let body = json!({ "reaction_type": { "emoji_type": kind } });
  api
    .post(&format!("/im/v1/messages/{id}/reactions"), &[], body)
    .await?;
  Ok(Action::done("react", &id).with_message(kind))
}

/// Take back the app's own reaction of this kind.
async fn unreact(api: &Api, id: &str, kind: &str) -> Result<()> {
  let creds = api.require()?;
  let query = [
    ("reaction_type", kind.to_owned()),
    ("page_size", "50".to_owned()),
  ];
  let v = api
    .get(&format!("/im/v1/messages/{id}/reactions"), &query)
    .await?;
  let ours = v.list("data.items").iter().find(|r| {
    r.str("operator.operator_type").as_deref() == Some("app")
      && r
        .str("operator.operator_id")
        .is_none_or(|op| op == creds.app_id)
  });
  let Some(reaction) = ours.and_then(|r| r.str("reaction_id")) else {
    return Err(Error::not_found(format!(
      "the bot has no {kind} reaction on {id}"
    )));
  };
  api
    .delete(&format!("/im/v1/messages/{id}/reactions/{reaction}"))
    .await?;
  Ok(())
}

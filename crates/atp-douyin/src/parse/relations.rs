//! Rows of your relation lists and the following search.

use atp_core::{Extra, User, Value, ValueExt};

use super::{put, user};
use crate::refs::user_url;

/// A row of a followers / following list, with the relation to you:
/// `mutual` (`follow_status` 2, or you follow them and they follow you),
/// `remark` (`remark_name`) and `special` (`special_follow_status`).
pub fn relation(v: &Value) -> Option<User> {
  let mut user = user(v)?;
  let (mine, theirs) = (v.i64("follow_status"), v.i64("follower_status"));
  if mine.is_some() || theirs.is_some() {
    let mutual = mine == Some(2) || (mine == Some(1) && theirs == Some(1));
    user.extra.insert("mutual".into(), mutual.into());
  }
  put(
    &mut user.extra,
    "remark",
    v.str("remark_name").map(Value::from),
  );
  if let Some(special) = v.i64("special_follow_status") {
    user.extra.insert("special".into(), (special == 1).into());
  }
  Some(user)
}

/// A user the at-mention suggestions offer (`suggest_words`, words with
/// a `rich_sug_*` JSON in `params.info`), with the relation they name:
/// `rich_sug_follow_status` is `follow`, `friends` (mutual), `fans` or
/// `not_related`.
pub fn suggested_user(word: &Value) -> Option<(User, String)> {
  let info: Value = serde_json::from_str(&word.str("params.info")?).ok()?;
  let sec = info.str("rich_sug_sec_uid")?;
  let status = info.str("rich_sug_follow_status").unwrap_or_default();
  let mut extra = Extra::new();
  put(&mut extra, "uid", word.str("id").map(Value::from));
  put(
    &mut extra,
    "remark",
    info.str("rich_sug_remarkname").map(Value::from),
  );
  extra.insert("mutual".into(), (status == "friends").into());
  let user = User {
    id: sec.clone(),
    name: info.str("rich_sug_nickname").unwrap_or_default(),
    handle: info
      .str("rich_sug_short_id")
      .filter(|h| !h.is_empty() && h != "0"),
    url: Some(user_url(&sec)),
    verified: info.str("rich_sug_cert_name").is_some(),
    followed: Some(status == "follow" || status == "friends"),
    extra,
    raw: Some(word.clone()),
    ..User::default()
  };
  Some((user, status))
}

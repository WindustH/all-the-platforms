//! Tags (标签): finding one by name, and following it.

use atp_core::{Action, Error, Result, ValueExt, json};

use crate::api::Api;

/// The `type` of a tag in `follow/do` and `follow/undo`.
const TAG_TYPE: u64 = 9;

/// The id of a tag named as on Juejin (`query_tag_list`, the editor's tag
/// search); a name without an exact match is refused with the closest ones.
pub async fn id(api: &Api, name: &str) -> Result<String> {
  let name = name.trim_start_matches('#').trim();
  let body = json!({ "cursor": "0", "key_word": name, "limit": 10, "sort_type": 1 });
  let v = api.editor("/tag_api/v1/query_tag_list", &body).await?;
  let tags: Vec<(String, String)> = v
    .list("data")
    .iter()
    .filter_map(|t| Some((t.str("tag.tag_id")?, t.str("tag.tag_name")?)))
    .collect();
  if let Some((id, _)) = tags.iter().find(|(_, n)| n.eq_ignore_ascii_case(name)) {
    return Ok(id.clone());
  }
  let close: Vec<&str> = tags.iter().map(|(_, n)| n.as_str()).collect();
  Err(Error::not_found(format!(
    "no Juejin tag named {name} (close: {})",
    if close.is_empty() {
      "none".to_owned()
    } else {
      close.join(", ")
    }
  )))
}

/// Follow (关注) a tag, by its id or name, or unfollow it, as the tag page
/// does.
pub async fn follow(api: &Api, tag: &str, undo: bool) -> Result<Action> {
  let tag = tag.trim();
  let id = if !tag.is_empty() && tag.bytes().all(|b| b.is_ascii_digit()) {
    tag.to_owned()
  } else {
    id(api, tag).await?
  };
  let (path, action) = if undo {
    ("/interact_api/v1/follow/undo", "unfollow")
  } else {
    ("/interact_api/v1/follow/do", "follow")
  };
  api
    .write(path, &json!({"id": id, "type": TAG_TYPE}))
    .await?;
  Ok(Action::done(action, id).with_message(format!("tag {tag}")))
}

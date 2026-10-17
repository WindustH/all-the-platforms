//! Changing a post or repost as web.okjike.com does (bundle `index-*.js`:
//! the post menu's 编辑动态, 转为仅自己可见 and 转为公开). The editor opens
//! only where the post says `editableStatus: ENABLED`, fills in what
//! `originalPosts/get` (or `reposts/get`) gives, and sends the post back
//! through `…/edit` with its pictures, video and link card as they were:
//! the text and the topic change. Who can see it goes through
//! `…/sponsor/hide` and `…/sponsor/recover`, as `privatizableStatus`
//! allows.

use atp_core::cli::Change;
use atp_core::{Action, Edit, Error, ErrorCode, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::posts;
use crate::refs::{self, Kind};

/// Change one of the account's posts: its text, topic or visibility.
pub async fn edit(api: &Api, arg: &str, edit: &Edit) -> Result<Action> {
  api.require_login()?;
  let (v, kind) = posts::fetch(api, &refs::post(arg)?).await?;
  let id = v.str("id").unwrap_or_default();
  let changes = edit.text.is_some() || edit.topics.is_some();
  if changes && v.str("editableStatus").as_deref() != Some("ENABLED") {
    return Err(denied("Jike does not offer to edit this post"));
  }
  let (private, target) = privacy(&v, edit.visibility.as_deref())?;
  let topic = match edit.topics.as_deref() {
    None => v.str("topic.id"),
    Some([]) => None,
    Some([one]) => Some(refs::topic(one)?),
    Some(_) => return Err(Error::input("a Jike post goes to one topic at most")),
  };
  let content = edit
    .text
    .clone()
    .unwrap_or_else(|| v.str("content").unwrap_or_default());
  let request = changes
    .then(|| body(&v, &content, topic.as_deref()))
    .transpose()?;
  ask(
    &v,
    (&content, topic.as_deref()),
    (private, target),
    edit.yes,
  )?;
  if let Some(body) = request {
    api
      .write(&format!("/1.0/{}/edit", kind.segment()), body)
      .await?;
  }
  if target != private {
    let verb = if target { "hide" } else { "recover" };
    let path = format!("/1.0/{}/sponsor/{verb}", kind.segment());
    api.write(&path, json!({ "id": id })).await?;
  }
  let mut done = Action::done("edit", &id).with_id(&id);
  if let Some(url) = url(&v, kind, &id) {
    done = done.with_url(url);
  }
  Ok(done)
}

/// Whether the post is private now and after the edit; a change must be
/// one the post menu offers (`privatizableStatus`).
fn privacy(v: &Value, visibility: Option<&str>) -> Result<(bool, bool)> {
  let private = v.bool("isPrivate") == Some(true);
  let target = match visibility {
    None => private,
    Some("private") => true,
    Some("public") => false,
    Some(other) => {
      return Err(Error::unsupported(format!(
        "edit --visibility {other} (Jike posts are public or private)"
      )));
    }
  };
  let allowed = if target {
    "ENABLE_HIDE"
  } else {
    "ENABLE_RECOVER"
  };
  if target != private && v.str("privatizableStatus").as_deref() != Some(allowed) {
    return Err(denied("Jike does not offer to change who sees this post"));
  }
  Ok((private, target))
}

/// The edit as the web editor sends it (`W_`): the kind of attachment,
/// the text, the pictures by key, the link card and the topic.
fn body(v: &Value, content: &str, topic: Option<&str>) -> Result<Value> {
  let pictures = v.list("pictures");
  let keys: Vec<String> = pictures.iter().filter_map(|p| p.str("key")).collect();
  if keys.len() != pictures.len() {
    return Err(Error::upstream(
      "Jike gave no key for a picture of this post; nothing was changed",
    ));
  }
  let link = v.at("linkInfo");
  let has_link = link.str("linkUrl").is_some();
  let edit_type = if v.at("video").is_object() {
    "VIDEO"
  } else if !keys.is_empty() {
    "IMAGE"
  } else if has_link {
    "LINK"
  } else {
    "TEXT"
  };
  if content.trim().is_empty() && edit_type == "TEXT" {
    return Err(Error::input("a Jike post without pictures needs a text"));
  }
  let mut body = json!({
    "id": v.str("id"),
    "editType": edit_type,
    "content": content,
    "pictureKeys": keys,
  });
  if has_link {
    let mut link = link.clone();
    link["supplementary"] = false.into();
    link["urlsInText"] = json!([]);
    link["linkType"] = "default".into();
    body["linkInfo"] = link;
  }
  if let Some(topic) = topic {
    body["submitToTopic"] = topic.into();
  }
  Ok(body)
}

/// Show the change and ask (or take `--yes`).
fn ask(v: &Value, new: (&str, Option<&str>), private: (bool, bool), yes: bool) -> Result<()> {
  let (content, topic) = new;
  let visibility = |private: bool| if private { "private" } else { "public" };
  let old_topic = v.str("topic.id");
  let topic_name = |id: Option<&str>| match id {
    Some(id) if Some(id) == old_topic.as_deref() => {
      v.str("topic.content").unwrap_or_else(|| id.to_owned())
    }
    Some(id) => format!("topic:{id}"),
    None => String::new(),
  };
  let mut change = Change::new(format!("Jike post {}:", v.str("id").unwrap_or_default()));
  change
    .field("text", &v.str("content").unwrap_or_default(), content)
    .field(
      "topic",
      &topic_name(old_topic.as_deref()),
      &topic_name(topic),
    )
    .field("visibility", visibility(private.0), visibility(private.1));
  let pictures = v.list("pictures").len();
  if pictures > 0 {
    change.row("pictures", &format!("{pictures} (kept)"));
  }
  if v.at("video").is_object() {
    change.row("video", "kept");
  }
  change.confirm(yes, "change this post on Jike")
}

fn url(v: &Value, kind: Kind, id: &str) -> Option<String> {
  let user = v.str("user.username")?;
  Some(refs::post_url(&user, kind, id))
}

fn denied(message: &str) -> Error {
  Error::new(ErrorCode::PermissionDenied, message)
    .with_hint("only your own posts change, and only where Jike's menu offers it")
}

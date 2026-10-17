//! Changing a post as weibo.com does (bundle `index-*.js`: the post menu's
//! `edit` and `modifyVisible`, the publisher's `modify`). Editing is for
//! members and takes text and pictures only: `checkCanEdit` says whether
//! the post may change and gives its text and pictures,
//! `user/count_limit` the edits left, and `modify` sends the new text
//! with the pictures as they were. Who can see a post changes through
//! `modifyVisible`, which needs no membership.

use atp_core::cli::Change;
use atp_core::{Action, Edit, Error, ErrorCode, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::posts;
use crate::refs::{self, post_url};

/// A post's text and pictures as the editor gets them.
struct Editable {
  text: String,
  /// `pic_id` of `modify`: the pictures as they are.
  pictures: Vec<Value>,
  /// Edits left (`count_limit`), when Weibo says.
  left: Option<u64>,
  status: Value,
}

/// Change one of the account's posts: its text, who can see it, or both.
pub async fn edit(api: &Api, arg: &str, edit: &Edit) -> Result<Action> {
  api.require_login()?;
  let mid = refs::mid(arg)?;
  let post = posts::show(api, &mid).await?;
  let visible = post.i64("visible.type").unwrap_or(0);
  if edit.text.is_some() && !matches!(visible, 0 | 1 | 6 | 10) {
    return Err(Error::unsupported(
      "edit of posts shown to a group or to paying fans",
    ));
  }
  let target = edit.visibility.as_deref().map(visibility_code);
  let editable = match &edit.text {
    Some(_) => Some(editable(api, &mid, &post).await?),
    None => None,
  };
  let mut change = Change::new(format!("Weibo post {mid}:"));
  if let (Some(text), Some(e)) = (&edit.text, &editable) {
    change.field("text", &e.text, text);
    if !e.pictures.is_empty() {
      change.row("pictures", &format!("{} (kept)", e.pictures.len()));
    }
  }
  let new_visible = target.map_or(visible, |(_, kind)| kind);
  change.field(
    "visibility",
    &visibility_name(visible),
    &visibility_name(new_visible),
  );
  if let Some(left) = editable.as_ref().and_then(|e| e.left) {
    change.row("edits", &format!("{left} left"));
  }
  change.confirm(edit.yes, "change this post on Weibo")?;
  if let (Some(text), Some(e)) = (&edit.text, &editable) {
    modify(api, &mid, text, e, visible).await?;
  }
  if let Some((code, _)) = target.filter(|(_, kind)| *kind != visible) {
    api.write_pause().await;
    let body = json!({ "ids": mid, "visible": code });
    api.post_json("/ajax/statuses/modifyVisible", &body).await?;
  }
  let uid = post.str("user.idstr");
  let url = post_url(uid.as_deref(), &mid, post.str("mblogid").as_deref());
  Ok(Action::done("edit", &mid).with_id(&mid).with_url(url))
}

/// Whether the post may change now (`checkCanEdit`) and how many edits
/// are left (`count_limit`), as the post menu asks before it opens the
/// editor.
async fn editable(api: &Api, mid: &str, post: &Value) -> Result<Editable> {
  let check = api
    .get("/ajax/statuses/checkCanEdit", &[("mid", mid)])
    .await?;
  if let Some(tip) = check.str("tip_message") {
    return Err(Error::new(
      ErrorCode::PermissionDenied,
      format!("Weibo: {tip}"),
    ));
  }
  let limit = api.get("/ajax/statuses/user/count_limit", &[]).await?;
  let quota = limit.at("result.edit");
  let left = quota.u64("residue_num");
  let blank = |key: &str| quota.at(key).as_str() == Some("");
  if left == Some(0) && !(blank("icon") && blank("text")) {
    return Err(
      Error::new(ErrorCode::PermissionDenied, "no Weibo edits left")
        .with_hint("editing posts is for Weibo members (VIP), with a number of edits a day"),
    );
  }
  let status = match check.at("status") {
    Value::Object(_) => check.at("status").clone(),
    _ => post.clone(),
  };
  let media = status.list("mix_media_info.items").len() + post.list("mix_media_info.items").len();
  let video = post
    .str("page_info.object_type")
    .is_some_and(|kind| kind.to_lowercase().contains("video"));
  if media > 0 || video {
    return Err(Error::unsupported(
      "edit of posts with videos or mixed media (Weibo edits text and pictures)",
    ));
  }
  Ok(Editable {
    text: status
      .first_str(&["text", "longText.longTextContent", "text_raw"])
      .or_else(|| post.first_str(&["longText.longTextContent", "text_raw"]))
      .unwrap_or_default(),
    pictures: pictures(&status, post)?,
    left: left.filter(|n| *n > 0),
    status,
  })
}

/// `pic_id` entries for the post's pictures, as the publisher sends them.
fn pictures(status: &Value, post: &Value) -> Result<Vec<Value>> {
  let source = if status.list("pic_ids").is_empty() {
    post
  } else {
    status
  };
  let mut pictures = Vec::new();
  for pid in source.list("pic_ids").iter().filter_map(Value::as_str) {
    let kind = source
      .str(&format!("pic_infos.{pid}.type"))
      .unwrap_or_else(|| "pic".into());
    if kind == "livephoto" {
      return Err(Error::unsupported("edit of posts with live photos"));
    }
    pictures.push(json!({ "type": kind, "pid": pid }));
  }
  Ok(pictures)
}

/// Send the new text (`modify`) with the pictures as they were.
/// `visible` is the post's own (`visible.type`): the edit page sends one,
/// and a post must not turn public by being edited.
async fn modify(api: &Api, mid: &str, text: &str, e: &Editable, visible: i64) -> Result<()> {
  let visible = visible.to_string();
  let pic_id = if e.pictures.is_empty() {
    String::new()
  } else {
    Value::from(e.pictures.clone()).to_string()
  };
  let mut form = vec![
    ("content", text.to_owned()),
    ("visible", visible),
    ("share_id", String::new()),
    ("mid", mid.to_owned()),
  ];
  if !pic_id.is_empty() {
    form.push(("pic_id", pic_id));
    form.push(("need_transcode", "1".into()));
  }
  if let Some(markdown) = e
    .status
    .bool("isMarkdown")
    .or_else(|| e.status.bool("is_markdown"))
  {
    form.push(("is_markdown", if markdown { "1" } else { "0" }.into()));
  }
  match e.status.at("blog_audio") {
    Value::Null => {}
    Value::String(audio) => form.push(("blog_audio", audio.clone())),
    audio => form.push(("blog_audio", audio.to_string())),
  }
  api.write_pause().await;
  let pairs: Vec<(&str, &str)> = form.iter().map(|(k, v)| (*k, v.as_str())).collect();
  let v = api.post_form("/ajax/statuses/modify", &pairs).await?;
  if let Some(message) = v.str("error").or_else(|| v.str("data.error")) {
    return Err(Error::upstream(message));
  }
  Ok(())
}

/// `modifyVisible`'s code for a visibility, and the `visible.type` a post
/// then has (its friends circle is 2 to the one, 6 to the other).
fn visibility_code(name: &str) -> (&'static str, i64) {
  match name {
    "private" => ("1", 1),
    "friends" => ("2", 6),
    _ => ("0", 0),
  }
}

fn visibility_name(kind: i64) -> String {
  match kind {
    0 => "public".into(),
    1 => "private".into(),
    6 => "friends".into(),
    10 => "fans".into(),
    5 => "group".into(),
    other => format!("visibility {other}"),
  }
}

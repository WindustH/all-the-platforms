//! Writes, as weibo.com's own composer sends them: posts with pictures
//! (uploaded to picupload first), reposts, comments and replies, likes,
//! favorites, follows and deletions.

use std::path::Path;

use social_core::file::Image;
use social_core::{Action, Draft, Error, Reply, Result, Value, ValueExt, json};

use crate::api::Client;
use crate::people;
use crate::refs::{self, mid_to_bid, post_url};

const UPLOAD: &str = "https://picupload.weibo.com/interface/pic_upload.php";
/// Pictures one post can carry.
const MAX_PICS: usize = 18;

/// Upload one picture; its `pid`.
async fn upload(c: &Client, path: &Path) -> Result<String> {
  let image = Image::read(path).await?;
  let resp = c
    .ctx
    .http
    .post(UPLOAD)
    .query("app", "miniblog")
    .query("s", "json")
    .query("p", 1)
    .query("data", 1)
    .header("referer", crate::api::HOME)
    .header("origin", "https://weibo.com")
    .bytes(image.data, "application/octet-stream")
    .send()
    .await?;
  let text = resp.text();
  let body = text.find('{').map_or(text.as_str(), |i| &text[i..]);
  let v: Value = serde_json::from_str(body).unwrap_or_default();
  if v.str("code").as_deref() != Some("A00006") {
    return Err(Error::upstream(format!(
      "picture upload failed ({}): {}",
      v.str("code").unwrap_or_else(|| resp.status.to_string()),
      path.display()
    )));
  }
  v.str("data.pics.pic_1.pid")
    .ok_or_else(|| Error::upstream("picture upload returned no pid"))
}

/// `#topic#` for every `--topic` the text does not already carry.
fn with_topics(text: &str, topics: &[String]) -> String {
  let mut out = text.to_owned();
  for t in topics {
    let tag = format!("#{}#", t.trim().trim_matches('#'));
    if !out.contains(&tag) {
      if !out.is_empty() {
        out.push(' ');
      }
      out.push_str(&tag);
    }
  }
  out
}

pub async fn publish(c: &Client, draft: &Draft) -> Result<Action> {
  c.require_login()?;
  if draft.reply_to.is_some() {
    return Err(Error::unsupported("post --reply-to").with_hint("use `comment` to reply"));
  }
  if let Some(quoted) = &draft.quote {
    if !draft.images.is_empty() {
      return Err(Error::unsupported("post --quote with --image"));
    }
    let text = with_topics(&draft.text, &draft.topics);
    return repost(c, quoted, &text, false).await;
  }
  if draft.images.len() > MAX_PICS {
    return Err(Error::input(format!(
      "Weibo takes at most {MAX_PICS} pictures"
    )));
  }
  let mut pics = Vec::new();
  for path in &draft.images {
    pics.push(json!({"type": "pic", "pid": upload(c, path).await?}));
  }
  let text = with_topics(&draft.text, &draft.topics);
  let content = match &draft.title {
    Some(title) if !text.is_empty() => format!("{title}\n{text}"),
    Some(title) => title.clone(),
    None => text,
  };
  if content.trim().is_empty() && pics.is_empty() {
    return Err(Error::input("nothing to post"));
  }
  let pic_id = if pics.is_empty() {
    String::new()
  } else {
    Value::from(pics).to_string()
  };
  c.write_pause().await;
  let v = c
    .post_form(
      "/ajax/statuses/update",
      &[
        ("content", &content),
        ("pic_id", &pic_id),
        ("visible", "0"),
        ("share_id", ""),
        ("media", ""),
        ("vote", ""),
      ],
    )
    .await?;
  created(&v, "publish", None)
}

/// The created post of a write response (`data` or the top level).
fn created(v: &Value, action: &str, target: Option<&str>) -> Result<Action> {
  if let Some(msg) = v.str("error").or_else(|| v.str("data.error")) {
    return Err(Error::upstream(msg));
  }
  let data = if v.at("data").is_object() {
    v.at("data")
  } else {
    v
  };
  let id = data.first_str(&["idstr", "mid", "id"]);
  let mut a = Action::done(
    action,
    target.map(str::to_owned).or(id.clone()).unwrap_or_default(),
  );
  if let Some(id) = id {
    let uid = data.str("user.idstr");
    let bid = data.str("mblogid").or_else(|| mid_to_bid(&id));
    a = a
      .with_url(post_url(uid.as_deref(), &id, bid.as_deref()))
      .with_id(id);
  }
  if let Some(msg) = v.str("msg").filter(|m| !m.is_empty()) {
    a = a.with_message(msg);
  }
  Ok(a)
}

/// Repost (转发) with an optional text; `also_comment` posts it as a comment too.
pub async fn repost(c: &Client, arg: &str, text: &str, also_comment: bool) -> Result<Action> {
  c.require_login()?;
  let mid = refs::mid(arg)?;
  c.write_pause().await;
  let v = c
    .post_form(
      "/ajax/statuses/normal_repost",
      &[
        ("id", &mid),
        ("comment", text),
        ("pic_id", ""),
        ("is_repost", "0"),
        ("comment_ori", "0"),
        ("is_comment", if also_comment { "1" } else { "0" }),
      ],
    )
    .await?;
  created(&v, "repost", Some(&mid))
}

pub async fn comment(c: &Client, arg: &str, reply: &Reply) -> Result<Action> {
  c.require_login()?;
  let mid = refs::mid(arg)?;
  if reply.text.trim().is_empty() {
    return Err(Error::input("empty comment"));
  }
  c.write_pause().await;
  let mut form = vec![
    ("id", mid.as_str()),
    ("comment", reply.text.as_str()),
    ("pic_id", ""),
    ("is_repost", "0"),
    ("comment_ori", "0"),
    ("is_comment", "0"),
  ];
  let (path, action) = match &reply.reply_to {
    Some(cid) => {
      form.push(("cid", cid.as_str()));
      ("/ajax/comments/reply", "reply")
    }
    None => ("/ajax/comments/create", "comment"),
  };
  let v = c.post_form(path, &form).await?;
  if let Some(msg) = v.str("error") {
    return Err(Error::upstream(msg));
  }
  let data = if v.at("data").is_object() {
    v.at("data")
  } else {
    &v
  };
  let mut a = Action::done(action, &mid).with_url(post_url(None, &mid, None));
  if let Some(id) = data.first_str(&["idstr", "id"]) {
    a = a.with_id(id);
  }
  Ok(a)
}

pub async fn delete_comment(c: &Client, comment: &str) -> Result<Action> {
  c.require_login()?;
  let cid = comment.trim();
  c.write_pause().await;
  c.post_json(
    "/ajax/statuses/destroyComment",
    &json!({"cid": cid, "filter": {}}),
  )
  .await?;
  Ok(Action::done("delete-comment", cid))
}

pub async fn like(c: &Client, arg: &str, undo: bool) -> Result<Action> {
  c.require_login()?;
  let mid = refs::mid(arg)?;
  let path = if undo {
    "/ajax/statuses/cancelLike"
  } else {
    "/ajax/statuses/setLike"
  };
  c.write_pause().await;
  c.post_json(path, &json!({"id": mid})).await?;
  Ok(Action::done(if undo { "unlike" } else { "like" }, &mid))
}

pub async fn favorite(c: &Client, arg: &str, folder: Option<&str>, undo: bool) -> Result<Action> {
  c.require_login()?;
  if folder.is_some() {
    return Err(Error::unsupported("favorite --folder").with_hint(
      "Weibo files favorites into folders (tags) on weibo.com; `favorites --folder` reads them",
    ));
  }
  let mid = refs::mid(arg)?;
  let path = if undo {
    "/ajax/statuses/destoryFavorites"
  } else {
    "/ajax/statuses/createFavorites"
  };
  c.write_pause().await;
  c.post_json(path, &json!({"id": mid})).await?;
  Ok(Action::done(
    if undo { "unfavorite" } else { "favorite" },
    &mid,
  ))
}

pub async fn follow(c: &Client, arg: &str, undo: bool) -> Result<Action> {
  c.require_login()?;
  let uid = people::uid(c, arg).await?;
  c.write_pause().await;
  if undo {
    c.post_json("/ajax/friendships/destory", &json!({"uid": uid}))
      .await?;
  } else {
    c.post_json("/ajax/friendships/create", &json!({"friend_uid": uid}))
      .await?;
  }
  Ok(Action::done(if undo { "unfollow" } else { "follow" }, &uid).with_url(refs::user_url(&uid)))
}

pub async fn delete(c: &Client, arg: &str) -> Result<Action> {
  c.require_login()?;
  let mid = refs::mid(arg)?;
  c.write_pause().await;
  c.post_json("/ajax/statuses/destroy", &json!({"id": mid}))
    .await?;
  Ok(Action::done("delete", &mid))
}

//! Messages (Mattermost posts): channel history, one post, threads (as
//! comments), sending with files and in threads, editing, deleting,
//! reactions, pins and flags.

use std::path::{Path, PathBuf};

use atp_core::file::{Upload, sniff};
use atp_core::http::Part;
use atp_core::text::emoji_shortcode;
use atp_core::{
  Action, Comment, Error, Media, Message, Page, PageReq, Post, Result, Value, ValueExt, json,
};

use crate::api::Api;
use crate::chats;
use crate::parse;
use crate::refs;
use crate::users;

/// Posts as the shared model, authors and channels resolved first.
pub async fn posts(api: &Api, rows: &[Value]) -> Result<Vec<Post>> {
  prepare(api, rows).await;
  chats::render(api, |view| {
    rows.iter().map(|p| parse::post(p, view)).collect()
  })
  .await
}

async fn prepare(api: &Api, rows: &[Value]) {
  users::resolve(api, rows.iter().filter_map(|p| p.str("user_id"))).await;
  let mut channels: Vec<String> = rows.iter().filter_map(|p| p.str("channel_id")).collect();
  channels.sort();
  channels.dedup();
  for c in channels {
    chats::info(api, &c).await;
  }
}

/// A channel's posts, newest first, paged by `before=`.
pub async fn list(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let channel = chats::channel_id(api, arg).await?;
  let per_page = page.size_within(200);
  let mut query = vec![("per_page", per_page.to_string())];
  if let Some(before) = &page.cursor {
    query.push(("before", before.clone()));
  }
  let v = api
    .get(&format!("/channels/{channel}/posts"), &query)
    .await?;
  let rows = parse::ordered(&v);
  let next = (rows.len() >= per_page)
    .then(|| rows.last().and_then(|p| p.str("id")))
    .flatten();
  Ok(Page::new(posts(api, &rows).await?, next))
}

async fn payload(api: &Api, arg: &str) -> Result<Value> {
  let id = refs::post_id(arg)?;
  api.get(&format!("/posts/{id}"), &[]).await
}

pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let v = payload(api, arg).await?;
  Ok(posts(api, &[v]).await?.remove(0))
}

pub async fn media(api: &Api, arg: &str) -> Result<(Post, Vec<Media>)> {
  let post = read(api, arg).await?;
  if post.media.is_empty() {
    return Err(Error::not_found("the post has no files"));
  }
  let media = post.media.clone();
  Ok((post, media))
}

/// The thread a post belongs to, oldest first, without its root.
pub async fn thread(api: &Api, arg: &str, _page: &PageReq) -> Result<Page<Comment>> {
  let v = payload(api, arg).await?;
  let root = root_of(&v);
  let list = api.get(&format!("/posts/{root}/thread"), &[]).await?;
  let mut rows: Vec<Value> = parse::ordered(&list)
    .into_iter()
    .filter(|p| p.str("id").as_deref() != Some(root.as_str()))
    .collect();
  rows.sort_by_key(|p| p.i64("create_at").unwrap_or_default());
  prepare(api, &rows).await;
  let items = chats::render(api, |view| {
    rows.iter().map(|p| parse::comment(p, view)).collect()
  })
  .await?;
  Ok(Page::last(items))
}

fn root_of(v: &Value) -> String {
  v.str("root_id")
    .filter(|r| !r.is_empty())
    .or_else(|| v.str("id"))
    .unwrap_or_default()
}

// ── writing ─────────────────────────────────────────────────────────────

pub async fn send(api: &Api, arg: &str, message: &Message) -> Result<Action> {
  let channel = chats::channel_id(api, arg).await?;
  let root = match message.reply_to.as_deref() {
    Some(r) => Some(root_of(&payload(api, r).await?)),
    None => None,
  };
  create(
    api,
    &channel,
    &message.text,
    root.as_deref(),
    &message.files,
  )
  .await
}

/// `comment`: a reply in the post's thread.
pub async fn reply(api: &Api, arg: &str, text: &str, files: &[PathBuf]) -> Result<Action> {
  let v = payload(api, arg).await?;
  let channel = v.str("channel_id").unwrap_or_default();
  create(api, &channel, text, Some(&root_of(&v)), files).await
}

async fn create(
  api: &Api,
  channel: &str,
  text: &str,
  root: Option<&str>,
  files: &[PathBuf],
) -> Result<Action> {
  let mut file_ids = Vec::new();
  for path in files {
    file_ids.push(upload(api, channel, path).await?);
  }
  let mut body = json!({"channel_id": channel, "message": text});
  if let Some(r) = root {
    body["root_id"] = json!(r);
  }
  if !file_ids.is_empty() {
    body["file_ids"] = json!(file_ids);
  }
  let v = api.post("/posts", body).await?;
  let id = v.str("id").unwrap_or_default();
  let url = chats::render(api, |view| view.post_url(channel, &id)).await?;
  let mut action = Action::done("send", channel).with_id(&id);
  if let Some(u) = url {
    action = action.with_url(u);
  }
  Ok(action)
}

/// Upload a file into a channel; its file id. Names without an extension
/// get the one their content shows.
async fn upload(api: &Api, channel: &str, path: &Path) -> Result<String> {
  let file = Upload::read(path).await?;
  let mut name = file.name.clone();
  if Path::new(&name).extension().is_none()
    && let Some(ext) = sniff(&file.data)
  {
    name = format!("{name}.{ext}");
  }
  let v = api
    .upload(
      "/files",
      vec![
        Part::text("channel_id", channel),
        Part::file("files", file.data, &name, file.mime),
      ],
    )
    .await?;
  v.str("file_infos.0.id")
    .ok_or_else(|| Error::upstream("the upload returned no file id"))
}

pub async fn edit(api: &Api, arg: &str, text: &str) -> Result<Action> {
  let id = refs::post_id(arg)?;
  api
    .put(&format!("/posts/{id}/patch"), json!({"message": text}))
    .await?;
  Ok(Action::done("edit", id))
}

pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  let id = refs::post_id(arg)?;
  api.delete(&format!("/posts/{id}")).await?;
  Ok(Action::done("delete", id))
}

pub async fn react(api: &Api, arg: &str, emoji: &str, undo: bool) -> Result<Action> {
  let id = refs::post_id(arg)?;
  let me = api.me().await?;
  let name = emoji_shortcode(emoji);
  if undo {
    api
      .delete(&format!("/users/{me}/posts/{id}/reactions/{name}"))
      .await?;
  } else {
    api
      .post(
        "/reactions",
        json!({"user_id": me, "post_id": id, "emoji_name": name}),
      )
      .await?;
  }
  let action = if undo { "unreact" } else { "react" };
  Ok(Action::done(action, id).with_message(format!(":{name}:")))
}

pub async fn pins(api: &Api, arg: &str) -> Result<Page<Post>> {
  let channel = chats::channel_id(api, arg).await?;
  let v = api.get(&format!("/channels/{channel}/pinned"), &[]).await?;
  Ok(Page::last(posts(api, &parse::ordered(&v)).await?))
}

pub async fn pin(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let id = refs::post_id(arg)?;
  let verb = if undo { "unpin" } else { "pin" };
  api
    .post(&format!("/posts/{id}/{verb}"), Value::Null)
    .await?;
  Ok(Action::done(verb, id))
}

/// Flagged (saved) posts; only your own are visible.
pub async fn favorites(api: &Api, user: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  if user.is_some_and(|u| u != "me") {
    return Err(
      Error::unsupported("favorites USER").with_hint("only your own flagged posts are visible"),
    );
  }
  flagged(api, page).await
}

async fn flagged(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  let me = api.me().await?;
  let per_page = page.size_within(200);
  let no = page.number_or(0);
  let v = api
    .get(
      &format!("/users/{me}/posts/flagged"),
      &[("page", no.to_string()), ("per_page", per_page.to_string())],
    )
    .await?;
  let rows = parse::ordered(&v);
  let next = (rows.len() >= per_page).then(|| (no + 1).to_string());
  Ok(Page::new(posts(api, &rows).await?, next))
}

pub async fn flag(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let id = refs::post_id(arg)?;
  let me = api.me().await?;
  let pref = json!([{"user_id": me, "category": "flagged_post", "name": id, "value": "true"}]);
  if undo {
    api
      .post(&format!("/users/{me}/preferences/delete"), pref)
      .await?;
  } else {
    api.put(&format!("/users/{me}/preferences"), pref).await?;
  }
  Ok(Action::done(if undo { "unflag" } else { "flag" }, id))
}

//! Channels, direct chats, history, sending (text and files), edits,
//! likes, deletions, members and users.

use std::cell::RefCell;
use std::path::Path;

use social_core::file::sniff;
use social_core::http::{Method, Part};
use social_core::{
  Action, Collection, Error, Media, Message, Page, PageReq, Post, Result, User, Value, ValueExt,
  json,
};

use crate::api::{Api, Body, Content, Mode};
use crate::parse::{self, Users};
use crate::refs::{self, Chat};

/// Users of the server, loaded once per run (user mode).
#[derive(Default)]
pub struct Directory {
  users: RefCell<Option<Users>>,
  me: RefCell<Option<i64>>,
}

impl Directory {
  pub async fn users(&self, api: &Api) -> Result<Users> {
    if let Some(u) = self.users.borrow().clone() {
      return Ok(u);
    }
    let server = api.server().unwrap_or_default();
    let users: Users = if api.mode() == Some(Mode::User) {
      api
        .get("/user")
        .await?
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|v| Some((v.i64("uid")?, parse::user(v, &server))))
        .collect()
    } else {
      Users::new()
    };
    *self.users.borrow_mut() = Some(users.clone());
    Ok(users)
  }

  pub async fn me(&self, api: &Api) -> Result<Option<i64>> {
    if api.mode() != Some(Mode::User) {
      return Ok(None);
    }
    if let Some(id) = *self.me.borrow() {
      return Ok(Some(id));
    }
    let id = api.get("/user/me").await?.i64("uid");
    *self.me.borrow_mut() = id;
    Ok(id)
  }
}

pub async fn whoami(api: &Api) -> Result<User> {
  let (server, mode) = api.require()?;
  if mode == Mode::Bot {
    let groups = api.get("/bot").await?;
    let mut u = User {
      id: "bot".into(),
      name: format!("bot key on {server}"),
      ..User::default()
    };
    u.extra.insert(
      "channels".into(),
      json!(groups.as_array().map(Vec::len).unwrap_or(0)),
    );
    return Ok(u);
  }
  Ok(parse::user(&api.get("/user/me").await?, &server))
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let server = api.server().unwrap_or_default();
  let uid = match refs::user(arg)? {
    Some(uid) => uid,
    None => return whoami(api).await,
  };
  let path = if api.mode() == Some(Mode::Bot) {
    format!("/bot/user/{uid}")
  } else {
    format!("/user/{uid}")
  };
  Ok(parse::user(&api.get(&path).await?, &server))
}

async fn name_of(api: &Api, dir: &Directory, chat: Chat) -> Result<String> {
  Ok(match chat {
    Chat::Group(g) => {
      let path = if api.mode() == Some(Mode::Bot) {
        format!("/bot/group/{g}")
      } else {
        format!("/group/{g}")
      };
      api.get(&path).await?.str("name").unwrap_or_default()
    }
    Chat::User(u) => dir
      .users(api)
      .await?
      .get(&u)
      .map(|u| u.name.clone())
      .unwrap_or_default(),
  })
}

/// Channels you are in, then direct chats with every other user.
pub async fn list(api: &Api, dir: &Directory) -> Result<Page<Collection>> {
  let (server, mode) = api.require()?;
  let path = if mode == Mode::Bot { "/bot" } else { "/group" };
  let mut out: Vec<Collection> = api
    .get(path)
    .await?
    .as_array()
    .cloned()
    .unwrap_or_default()
    .iter()
    .map(|g| parse::group(g, &server))
    .collect();
  let me = dir.me(api).await?;
  let mut users: Vec<User> = dir.users(api).await?.into_values().collect();
  users.sort_by_key(|a| a.name.to_lowercase());
  out.extend(
    users
      .iter()
      .filter(|u| Some(u.id.parse::<i64>().unwrap_or(-1)) != me)
      .map(parse::direct),
  );
  Ok(Page::last(out))
}

pub async fn history(api: &Api, dir: &Directory, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  api.require_user("messages")?;
  let chat = refs::chat(arg)?;
  history_of(
    api,
    dir,
    chat,
    page.cursor.as_deref(),
    page.size_within(100),
  )
  .await
}

async fn history_of(
  api: &Api,
  dir: &Directory,
  chat: Chat,
  before: Option<&str>,
  size: usize,
) -> Result<Page<Post>> {
  let server = api.server().unwrap_or_default();
  let mut path = format!("{}/history?limit={size}", chat.path());
  if let Some(b) = before {
    path.push_str(&format!("&before={b}"));
  }
  let rows = api
    .get(&path)
    .await?
    .as_array()
    .cloned()
    .unwrap_or_default();
  let users = dir.users(api).await?;
  let name = name_of(api, dir, chat).await.unwrap_or_default();
  let key = chat.key();
  // The server lists oldest first; show newest first like other platforms.
  let mut posts: Vec<Post> = rows
    .iter()
    .map(|m| parse::message(m, &key, &name, &users, &server))
    .collect();
  posts.reverse();
  let next = (rows.len() >= size)
    .then(|| {
      rows
        .first()
        .and_then(|m| m.i64("mid"))
        .map(|m| m.to_string())
    })
    .flatten();
  Ok(Page::new(parse::fold(posts), next))
}

pub async fn read(api: &Api, dir: &Directory, arg: &str) -> Result<Post> {
  api.require_user("read")?;
  let (chat, mid) = refs::message(arg)?;
  let page = history_of(api, dir, chat, Some(&(mid + 1).to_string()), 1).await?;
  page
    .items
    .into_iter()
    .find(|p| p.extra.get("mid").and_then(Value::as_i64) == Some(mid))
    .ok_or_else(|| Error::not_found(format!("no message {mid} in {}", chat.key())))
}

pub async fn media(api: &Api, dir: &Directory, arg: &str) -> Result<(Post, Vec<Media>)> {
  let post = read(api, dir, arg).await?;
  if post.media.is_empty() {
    return Err(Error::not_found("the message has no image, video or audio"));
  }
  let media = post.media.clone();
  Ok((post, media))
}

/// Upload a file (prepare + one chunk); its server path comes back.
async fn upload(api: &Api, path: &Path) -> Result<String> {
  let data = tokio::fs::read(path)
    .await
    .map_err(|e| Error::input(format!("cannot read {}: {e}", path.display())))?;
  let name = path
    .file_name()
    .and_then(|n| n.to_str())
    .unwrap_or("file")
    .to_owned();
  let mime = match sniff(&data) {
    Some("jpg") => "image/jpeg",
    Some("png") => "image/png",
    Some("gif") => "image/gif",
    Some("webp") => "image/webp",
    Some("mp4") => "video/mp4",
    _ => "application/octet-stream",
  };
  let base = if api.mode() == Some(Mode::Bot) {
    "/bot/file"
  } else {
    "/resource/file"
  };
  let id = api
    .call(
      Method::POST,
      &format!("{base}/prepare"),
      Body::Json(json!({"content_type": mime, "filename": name})),
    )
    .await?;
  let id = id
    .as_str()
    .map(str::to_owned)
    .ok_or_else(|| Error::upstream("the upload was not prepared (no file id)"))?;
  let v = api
    .call(
      Method::POST,
      &format!("{base}/upload"),
      Body::Multipart(vec![
        Part::text("file_id", id),
        Part::file("chunk_data", data, &name, mime),
        Part::text("chunk_is_last", "true"),
      ]),
    )
    .await?;
  v.str("path")
    .ok_or_else(|| Error::upstream("the upload returned no file path"))
}

async fn post_content(api: &Api, chat: Chat, reply: Option<i64>, content: Content) -> Result<i64> {
  let bot = api.mode() == Some(Mode::Bot);
  let path = match (reply, chat, bot) {
    (Some(_), _, true) => {
      return Err(Error::unsupported("send --reply-to").with_hint("VoceChat bots cannot reply"));
    }
    (Some(mid), _, false) => format!("/message/{mid}/reply"),
    (None, Chat::Group(g), true) => format!("/bot/send_to_group/{g}"),
    (None, Chat::User(u), true) => format!("/bot/send_to_user/{u}"),
    (None, c, false) => format!("{}/send", c.path()),
  };
  api
    .ctx
    .http
    .pause(
      std::time::Duration::from_millis(200),
      std::time::Duration::from_millis(600),
    )
    .await;
  let v = api.call(Method::POST, &path, content.into()).await?;
  Ok(v.as_i64().unwrap_or_default())
}

pub async fn send(api: &Api, arg: &str, message: &Message, markdown: bool) -> Result<Action> {
  api.require()?;
  let chat = refs::chat(arg)?;
  let reply = match &message.reply_to {
    Some(r) => Some(refs::message(r)?.1),
    None => None,
  };
  let mut first = None;
  if !message.text.is_empty() {
    let content = if markdown {
      Content::Markdown(message.text.clone())
    } else {
      Content::Text(message.text.clone())
    };
    first = Some(post_content(api, chat, reply, content).await?);
  }
  for f in &message.files {
    let path = upload(api, f).await?;
    let r = if first.is_none() { reply } else { None };
    let mid = post_content(api, chat, r, Content::File(path)).await?;
    first.get_or_insert(mid);
  }
  Ok(
    Action::done(if reply.is_some() { "reply" } else { "send" }, chat.key()).with_id(format!(
      "{}/{}",
      chat.key(),
      first.unwrap_or_default()
    )),
  )
}

pub async fn edit(api: &Api, arg: &str, text: &str) -> Result<Action> {
  api.require_user("edit")?;
  let (chat, mid) = refs::message(arg)?;
  api
    .call(
      Method::PUT,
      &format!("/message/{mid}/edit"),
      Content::Text(text.to_owned()).into(),
    )
    .await?;
  Ok(Action::done("edit", format!("{}/{mid}", chat.key())))
}

/// The emoji VoceChat accepts as reactions.
pub const EMOJI: &[&str] = &["❤️", "😄", "👀", "👍", "👎", "🎉", "🙁", "🚀"];

pub async fn react(api: &Api, arg: &str, emoji: &str, undo: bool) -> Result<Action> {
  api.require_user("react")?;
  let (chat, mid) = refs::message(arg)?;
  let e = emoji.trim();
  let e = if e == "❤" { "❤️" } else { e };
  if !EMOJI.contains(&e) {
    return Err(Error::input(format!(
      "VoceChat takes these reactions: {}",
      EMOJI.join(" ")
    )));
  }
  // The same reaction again takes it back.
  api
    .call(
      Method::PUT,
      &format!("/message/{mid}/like"),
      Body::Json(json!({"action": e})),
    )
    .await?;
  Ok(
    Action::done(
      if undo { "unreact" } else { "react" },
      format!("{}/{mid}", chat.key()),
    )
    .with_message(e),
  )
}

pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  api.require_user("delete")?;
  let (chat, mid) = refs::message(arg)?;
  api
    .call(Method::DELETE, &format!("/message/{mid}"), Body::None)
    .await?;
  Ok(Action::done("delete", format!("{}/{mid}", chat.key())))
}

/// Members of a channel (all users for public channels), or both people of
/// a direct chat.
pub async fn members(api: &Api, dir: &Directory, arg: &str) -> Result<Page<User>> {
  let server = api.server().unwrap_or_default();
  let users = dir.users(api).await?;
  let ids: Vec<i64> = match refs::chat(arg)? {
    Chat::Group(g) => {
      let path = if api.mode() == Some(Mode::Bot) {
        format!("/bot/group/{g}")
      } else {
        format!("/group/{g}")
      };
      let v: Value = api.get(&path).await?;
      let ids: Vec<i64> = v.list("members").iter().filter_map(Value::as_i64).collect();
      if ids.is_empty() && v.bool("is_public") == Some(true) {
        users.keys().copied().collect()
      } else {
        ids
      }
    }
    Chat::User(u) => dir.me(api).await?.into_iter().chain([u]).collect(),
  };
  let mut out = Vec::new();
  for id in ids {
    let u = match users.get(&id) {
      Some(u) => u.clone(),
      None if api.mode() == Some(Mode::Bot) => {
        parse::user(&api.get(&format!("/bot/user/{id}")).await?, &server)
      }
      None => User {
        id: id.to_string(),
        name: format!("user {id}"),
        ..User::default()
      },
    };
    out.push(u);
  }
  Ok(Page::last(out))
}

//! Bot accounts through the Bot API (`login --token 123:ABC`, or the
//! `TELEGRAM_BOT_TOKEN` variable). Bots can send, edit, delete, react, pin,
//! look up chats and admins and receive updates, but cannot list their chats
//! or read history: Telegram only hands them messages as updates.

use std::path::{Path, PathBuf};
use std::time::Duration;

use social_core::http::Part;
use social_core::output::Stream;
use social_core::text::from_secs;
use social_core::{
  Action, Ctx, Error, ErrorCode, Media, MediaKind, Message, Page, Post, Result, User, Value,
  ValueExt, json,
};

use crate::refs::{self, Chat};

const API: &str = "https://api.telegram.org";
/// Session extra holding the token.
pub const TOKEN: &str = "bot_token";

pub struct Bot {
  token: String,
}

impl Bot {
  pub fn from_ctx(ctx: &Ctx) -> Option<Self> {
    let token = ctx
      .extra(TOKEN)
      .filter(|t| !t.is_empty())
      .or_else(|| std::env::var("TELEGRAM_BOT_TOKEN").ok())
      .filter(|t| !t.trim().is_empty())?;
    Some(Self {
      token: token.trim().to_owned(),
    })
  }

  /// `123456:ABC…`, as @BotFather hands it out.
  pub fn valid_token(token: &str) -> bool {
    token.split_once(':').is_some_and(|(id, secret)| {
      !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) && secret.len() >= 20
    })
  }

  async fn call(&self, ctx: &Ctx, method: &str, body: Value) -> Result<Value> {
    let resp = ctx
      .http
      .post(format!("{API}/bot{}/{method}", self.token))
      .no_cookies()
      .json(&body)
      .send()
      .await?;
    answer(resp.value().unwrap_or_default(), resp.status.as_u16())
  }

  async fn call_long(&self, ctx: &Ctx, method: &str, body: Value, wait: Duration) -> Result<Value> {
    let resp = ctx
      .http
      .post(format!("{API}/bot{}/{method}", self.token))
      .no_cookies()
      .no_throttle()
      .timeout(wait)
      .json(&body)
      .send()
      .await?;
    answer(resp.value().unwrap_or_default(), resp.status.as_u16())
  }

  async fn call_parts(&self, ctx: &Ctx, method: &str, parts: Vec<Part>) -> Result<Value> {
    let resp = ctx
      .http
      .post(format!("{API}/bot{}/{method}", self.token))
      .no_cookies()
      .timeout(Duration::from_secs(300))
      .multipart(parts)
      .send()
      .await?;
    answer(resp.value().unwrap_or_default(), resp.status.as_u16())
  }

  pub async fn whoami(&self, ctx: &Ctx) -> Result<User> {
    let me = self.call(ctx, "getMe", json!({})).await?;
    Ok(user(&me))
  }

  /// A chat the bot can see, as a user (profiles of groups and channels too).
  pub async fn user(&self, ctx: &Ctx, arg: &str) -> Result<User> {
    let chat_id = chat_id(&refs::chat(arg)?)?;
    let chat = self
      .call(ctx, "getChat", json!({"chat_id": chat_id}))
      .await?;
    let mut out = chat_user(&chat);
    if chat.str("type").as_deref() != Some("private")
      && let Ok(n) = self
        .call(ctx, "getChatMemberCount", json!({"chat_id": chat_id}))
        .await
    {
      out.stats.followers = n.as_u64();
    }
    Ok(out)
  }

  /// The administrators: bots cannot list other members.
  pub async fn members(&self, ctx: &Ctx, chat: &str) -> Result<Page<User>> {
    let chat_id = chat_id(&refs::chat(chat)?)?;
    let admins = self
      .call(ctx, "getChatAdministrators", json!({"chat_id": chat_id}))
      .await?;
    let items = admins
      .as_array()
      .map(Vec::as_slice)
      .unwrap_or_default()
      .iter()
      .map(|m| {
        let mut u = user(m.at("user"));
        u.extra
          .insert("role".into(), m.str("status").unwrap_or_default().into());
        if let Some(title) = m.str("custom_title") {
          u.extra.insert("title".into(), title.into());
        }
        u
      })
      .collect();
    Ok(Page::last(items))
  }

  pub async fn send(&self, ctx: &Ctx, chat: &str, message: &Message) -> Result<Action> {
    let chat_ref = refs::chat(chat)?;
    let chat_id = chat_id(&chat_ref)?;
    let reply = match &message.reply_to {
      Some(r) => Some(refs::message(r)?.id),
      None => None,
    };
    let reply_parameters = reply.map(|id| json!({"message_id": id}));
    let sent = match message.files.as_slice() {
      [] => {
        let mut body = json!({"chat_id": chat_id, "text": message.text});
        if let Some(r) = reply_parameters {
          body["reply_parameters"] = r;
        }
        self.call(ctx, "sendMessage", body).await?
      }
      [file] => {
        let (method, field) = if is_photo(file) {
          ("sendPhoto", "photo")
        } else {
          ("sendDocument", "document")
        };
        let mut parts = vec![
          Part::text("chat_id", chat_id.to_string()),
          file_part(field, file).await?,
        ];
        if !message.text.is_empty() {
          parts.push(Part::text("caption", message.text.clone()));
        }
        if let Some(r) = reply_parameters {
          parts.push(Part::text("reply_parameters", r.to_string()));
        }
        self.call_parts(ctx, method, parts).await?
      }
      files => {
        let mut parts = vec![Part::text("chat_id", chat_id.to_string())];
        let mut media = Vec::new();
        for (i, file) in files.iter().enumerate() {
          let name = format!("file{i}");
          let kind = if is_photo(file) { "photo" } else { "document" };
          let mut item = json!({"type": kind, "media": format!("attach://{name}")});
          if i == 0 && !message.text.is_empty() {
            item["caption"] = message.text.clone().into();
          }
          media.push(item);
          parts.push(file_part(&name, file).await?);
        }
        parts.push(Part::text("media", Value::Array(media).to_string()));
        if let Some(r) = reply_parameters {
          parts.push(Part::text("reply_parameters", r.to_string()));
        }
        let sent = self.call_parts(ctx, "sendMediaGroup", parts).await?;
        sent.list("").first().cloned().unwrap_or(sent)
      }
    };
    let post = post(&sent);
    let mut action = Action::done("send", chat_ref.to_string()).with_id(&post.id);
    if let Some(url) = post.url {
      action = action.with_url(url);
    }
    Ok(action)
  }

  pub async fn edit(&self, ctx: &Ctx, arg: &str, text: &str) -> Result<Action> {
    let (chat_id, id) = message_ref(arg)?;
    self
      .call(
        ctx,
        "editMessageText",
        json!({"chat_id": chat_id, "message_id": id, "text": text}),
      )
      .await?;
    Ok(Action::done("edit", arg))
  }

  pub async fn delete(&self, ctx: &Ctx, arg: &str) -> Result<Action> {
    let (chat_id, id) = message_ref(arg)?;
    self
      .call(
        ctx,
        "deleteMessage",
        json!({"chat_id": chat_id, "message_id": id}),
      )
      .await?;
    Ok(Action::done("delete", arg))
  }

  pub async fn react(&self, ctx: &Ctx, arg: &str, emoji: &str, undo: bool) -> Result<Action> {
    let (chat_id, id) = message_ref(arg)?;
    let reaction = if undo {
      json!([])
    } else {
      json!([{"type": "emoji", "emoji": emoji}])
    };
    self
      .call(
        ctx,
        "setMessageReaction",
        json!({"chat_id": chat_id, "message_id": id, "reaction": reaction}),
      )
      .await?;
    Ok(Action::done(if undo { "unreact" } else { "react" }, arg).with_message(emoji))
  }

  pub async fn pin(&self, ctx: &Ctx, arg: &str, undo: bool) -> Result<Action> {
    let (chat_id, id) = message_ref(arg)?;
    let method = if undo {
      "unpinChatMessage"
    } else {
      "pinChatMessage"
    };
    self
      .call(ctx, method, json!({"chat_id": chat_id, "message_id": id}))
      .await?;
    Ok(Action::done(if undo { "unpin" } else { "pin" }, arg))
  }

  pub async fn forward(&self, ctx: &Ctx, arg: &str, to: &str) -> Result<Action> {
    let (from, id) = message_ref(arg)?;
    let to = chat_id(&refs::chat(to)?)?;
    let sent = self
      .call(
        ctx,
        "forwardMessage",
        json!({"chat_id": to, "from_chat_id": from, "message_id": id}),
      )
      .await?;
    let post = post(&sent);
    let mut action = Action::done("forward", arg).with_id(&post.id);
    if let Some(url) = post.url {
      action = action.with_url(url);
    }
    Ok(action)
  }

  /// Long-poll `getUpdates` and print each message, edit and reaction.
  pub async fn listen(&self, ctx: &Ctx, chats: &[String], out: &Stream) -> Result<()> {
    let wanted = chats
      .iter()
      .map(|c| refs::chat(c))
      .collect::<Result<Vec<_>>>()?;
    let mut offset: Option<i64> = None;
    loop {
      let mut body = json!({
        "timeout": 50,
        "allowed_updates": ["message", "edited_message", "channel_post", "edited_channel_post", "message_reaction_count"],
      });
      if let Some(o) = offset {
        body["offset"] = o.into();
      }
      let updates = self
        .call_long(ctx, "getUpdates", body, Duration::from_secs(70))
        .await
        .map_err(|e| {
          if e.message.contains("webhook") {
            e.with_hint("the bot has a webhook; remove it (deleteWebhook) to receive updates here")
          } else {
            e
          }
        })?;
      for u in updates.as_array().map(Vec::as_slice).unwrap_or_default() {
        offset = u.i64("update_id").map(|id| id + 1).or(offset);
        let Some((kind, message)) = [
          ("message", "message"),
          ("edit", "edited_message"),
          ("message", "channel_post"),
          ("edit", "edited_channel_post"),
          ("reactions", "message_reaction_count"),
        ]
        .into_iter()
        .find_map(|(kind, key)| u.get(key).map(|m| (kind, m))) else {
          continue;
        };
        if !wanted.is_empty() && !wanted.iter().any(|c| same_chat(c, message.at("chat"))) {
          continue;
        }
        let mut p = post(message);
        p.kind = kind.into();
        out.post(&p);
      }
    }
  }

  /// Fetch a `tg-bot-file:` media (at most 20 MB, a Bot API limit).
  pub async fn save(&self, ctx: &Ctx, media: &Media, dir: &Path, name: &str) -> Result<PathBuf> {
    let file_id = media
      .url
      .strip_prefix("tg-bot-file:")
      .ok_or_else(|| Error::internal("not a bot file"))?;
    let file = self
      .call(ctx, "getFile", json!({"file_id": file_id}))
      .await?;
    let path = file.str("file_path").ok_or_else(|| {
      Error::upstream("Telegram gave no file path (files over 20 MB cannot be fetched by bots)")
    })?;
    let ext = path
      .rsplit_once('.')
      .map(|(_, e)| e.to_owned())
      .unwrap_or_else(|| "bin".into());
    let out = dir.join(format!("{name}.{ext}"));
    let resp = ctx
      .http
      .get(format!("{API}/file/bot{}/{path}", self.token))
      .no_cookies()
      .timeout(Duration::from_secs(300))
      .send()
      .await?
      .check()?;
    tokio::fs::write(&out, &resp.body).await?;
    Ok(out)
  }
}

fn answer(v: Value, status: u16) -> Result<Value> {
  if v.bool("ok") == Some(true) {
    return Ok(v.get("result").cloned().unwrap_or(Value::Null));
  }
  let description = v
    .str("description")
    .unwrap_or_else(|| format!("HTTP {status}"));
  let text = format!("Telegram Bot API: {description}");
  Err(match v.i64("error_code").unwrap_or(status.into()) {
    401 => Error::auth(text).with_hint("the bot token was refused; get a new one from @BotFather"),
    403 => Error::new(ErrorCode::PermissionDenied, text),
    404 => Error::not_found(text),
    429 => Error::new(
      ErrorCode::RateLimited,
      format!(
        "{text} (retry after {} s)",
        v.i64("parameters.retry_after").unwrap_or(1)
      ),
    ),
    400 if description.contains("not found") => Error::not_found(text),
    400 => Error::input(text),
    _ => Error::upstream(text),
  })
}

/// `chat_id` for the Bot API: `@name` or the numeric id.
fn chat_id(chat: &Chat) -> Result<Value> {
  match chat {
    Chat::Username(u) => Ok(format!("@{u}").into()),
    Chat::Id(id) => Ok((*id).into()),
    Chat::Invite(_) => Err(Error::input(
      "bots cannot use invite links; use the chat's id or @name",
    )),
  }
}

fn message_ref(arg: &str) -> Result<(Value, i32)> {
  let m = refs::message(arg)?;
  Ok((chat_id(&m.chat)?, m.id))
}

fn same_chat(want: &Chat, chat: &Value) -> bool {
  match want {
    Chat::Id(id) => chat.i64("id") == Some(*id),
    Chat::Username(u) => chat
      .str("username")
      .is_some_and(|n| n.eq_ignore_ascii_case(u)),
    Chat::Invite(_) => false,
  }
}

fn is_photo(path: &Path) -> bool {
  let ext = path
    .extension()
    .and_then(|e| e.to_str())
    .map(str::to_ascii_lowercase)
    .unwrap_or_default();
  matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp")
}

async fn file_part(field: &str, path: &Path) -> Result<Part> {
  let data = tokio::fs::read(path)
    .await
    .map_err(|e| Error::input(format!("cannot read {}: {e}", path.display())))?;
  let name = path
    .file_name()
    .and_then(|n| n.to_str())
    .unwrap_or("file")
    .to_owned();
  let mime = social_core::file::sniff(&data)
    .map(|ext| match ext {
      "jpg" => "image/jpeg",
      "png" => "image/png",
      "gif" => "image/gif",
      "webp" => "image/webp",
      _ => "application/octet-stream",
    })
    .unwrap_or("application/octet-stream");
  Ok(Part::file(field, data, &name, mime))
}

/// A Bot API `User`.
fn user(u: &Value) -> User {
  let name = [u.str("first_name"), u.str("last_name")]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ");
  let handle = u.str("username");
  let mut out = User {
    id: u.str("id").unwrap_or_default(),
    name,
    url: handle.as_ref().map(|h| format!("https://t.me/{h}")),
    handle,
    ..User::default()
  };
  let kind = if u.bool("is_bot") == Some(true) {
    "bot"
  } else {
    "user"
  };
  out.extra.insert("kind".into(), kind.into());
  out
}

/// A Bot API `Chat` / `ChatFullInfo`, as a user.
fn chat_user(c: &Value) -> User {
  let handle = c.str("username");
  let name = c.str("title").unwrap_or_else(|| user(c).name);
  let mut out = User {
    id: c.str("id").unwrap_or_default(),
    name,
    url: handle.as_ref().map(|h| format!("https://t.me/{h}")),
    handle,
    bio: c.first_str(&["bio", "description"]),
    ..User::default()
  };
  out
    .extra
    .insert("kind".into(), c.str("type").unwrap_or_default().into());
  if let Some(linked) = c.i64("linked_chat_id") {
    out.extra.insert("linked_chat".into(), linked.into());
  }
  out
}

/// A Bot API `Message`; files point at `tg-bot-file:` for `download`.
pub fn post(m: &Value) -> Post {
  let chat = m.at("chat");
  let chat_id = chat.i64("id").unwrap_or_default();
  let username = chat.str("username");
  let id = m.i64("message_id").unwrap_or_default() as i32;
  let author = if m.get("from").is_some() {
    Some(user(m.at("from")))
  } else if m.get("sender_chat").is_some() {
    Some(chat_user(m.at("sender_chat")))
  } else {
    None
  };
  let mut post = Post {
    id: refs::message_id(username.as_deref(), chat_id, id),
    kind: "message".into(),
    text: m.first_str(&["text", "caption"]),
    url: refs::message_url(username.as_deref(), chat_id, id),
    author,
    created_at: m.i64("date").and_then(from_secs),
    updated_at: m.i64("edit_date").and_then(from_secs),
    ..Post::default()
  };
  post.media = files(m);
  let extra = &mut post.extra;
  extra.insert("chat".into(), chat_id.into());
  let chat_name = chat.first_str(&["title", "first_name"]).unwrap_or_default();
  extra.insert("chat_name".into(), chat_name.into());
  extra.insert("message_id".into(), id.into());
  if let Some(r) = m.i64("reply_to_message.message_id") {
    extra.insert(
      "reply_to".into(),
      refs::message_id(username.as_deref(), chat_id, r as i32).into(),
    );
  }
  if let Some(g) = m.str("media_group_id") {
    extra.insert("album".into(), g.into());
  }
  if let Some(origin) = m.get("forward_origin") {
    extra.insert("forwarded_from".into(), origin.clone());
  }
  let counts: Vec<Value> = m
    .list("reactions")
    .iter()
    .map(
      |r| json!({"emoji": r.str("type.emoji").unwrap_or_default(), "count": r.u64("total_count")}),
    )
    .collect();
  if !counts.is_empty() {
    post.metrics.likes = Some(counts.iter().filter_map(|c| c["count"].as_u64()).sum());
    post.extra.insert("reactions".into(), Value::Array(counts));
  }
  post.raw = Some(m.clone());
  post
}

fn files(m: &Value) -> Vec<Media> {
  let file = |kind: MediaKind, v: &Value| {
    let mut media = Media::new(kind, format!("tg-bot-file:{}", v.str("file_id")?));
    media.size = v.u64("file_size");
    media.width = v.u64("width").map(|n| n as u32);
    media.height = v.u64("height").map(|n| n as u32);
    media.duration = v.f64("duration");
    media.alt = v.str("file_name");
    Some(media)
  };
  let mut out = Vec::new();
  // The largest size of a photo comes last.
  if let Some(photo) = m.list("photo").last() {
    out.extend(file(MediaKind::Image, photo));
  }
  for (key, kind) in [
    ("video", MediaKind::Video),
    ("animation", MediaKind::Gif),
    ("audio", MediaKind::Audio),
    ("voice", MediaKind::Audio),
    ("video_note", MediaKind::Video),
    ("sticker", MediaKind::Image),
    ("document", MediaKind::File),
  ] {
    if let Some(v) = m.get(key) {
      // An animation also comes as a document.
      if key == "document" && m.get("animation").is_some() {
        continue;
      }
      out.extend(file(kind, v));
    }
  }
  out
}

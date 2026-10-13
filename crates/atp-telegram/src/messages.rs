//! Messages on whichever route is active: history (public channels through
//! the web preview without an account), single messages, comments, sending,
//! editing, deleting, reactions, pins, forwards and media files.

use std::path::{Path, PathBuf};

use atp_core::{Action, Comment, Media, Message, Page, PageReq, Post, Result};

use crate::api::{Api, Mode, bots_cannot};
use crate::refs::{self, Chat};
use crate::{bot::Bot, mt, web};

/// A chat's messages, newest first.
pub async fn list(api: &Api, chat: &str, page: &PageReq) -> Result<Page<Post>> {
  match api.mode()? {
    Mode::User(_) => {
      api
        .as_user(async |c| mt::messages(c, chat, page).await)
        .await
    }
    Mode::Bot(_) | Mode::Web => public_posts(api, chat, page).await,
  }
}

/// A public channel's posts, for routes without history.
async fn public_posts(api: &Api, chat: &str, page: &PageReq) -> Result<Page<Post>> {
  match refs::chat(chat)? {
    Chat::Username(u) => web::posts(&api.ctx, &u, None, page.cursor.as_deref()).await,
    _ => Err(api.needs_account("reading this chat")),
  }
}

pub async fn read(api: &Api, post: &str) -> Result<Post> {
  match api.mode()? {
    Mode::User(_) => api.as_user(async |c| mt::read(c, post).await).await,
    Mode::Bot(_) | Mode::Web => {
      let message = refs::message(post)?;
      match &message.chat {
        Chat::Username(u) => web::post(&api.ctx, u, message.id).await,
        _ => Err(api.needs_account("reading this message")),
      }
    }
  }
}

pub async fn comments(api: &Api, post: &str, page: &PageReq) -> Result<Page<Comment>> {
  match api.mode()? {
    Mode::User(_) => {
      api
        .as_user(async |c| mt::comments(c, post, page).await)
        .await
    }
    Mode::Bot(_) => Err(bots_cannot("comments")),
    Mode::Web => Err(api.needs_account("reading comments")),
  }
}

pub async fn send(api: &Api, chat: &str, message: &Message) -> Result<Action> {
  match api.mode()? {
    Mode::User(_) => {
      api
        .as_user(async |c| mt::send(c, chat, message).await)
        .await
    }
    Mode::Bot(b) => b.send(&api.ctx, chat, message).await,
    Mode::Web => Err(api.needs_account("sending")),
  }
}

pub async fn edit(api: &Api, post: &str, text: &str) -> Result<Action> {
  match api.mode()? {
    Mode::User(_) => api.as_user(async |c| mt::edit(c, post, text).await).await,
    Mode::Bot(b) => b.edit(&api.ctx, post, text).await,
    Mode::Web => Err(api.needs_account("editing")),
  }
}

pub async fn delete(api: &Api, post: &str) -> Result<Action> {
  match api.mode()? {
    Mode::User(_) => api.as_user(async |c| mt::delete(c, post).await).await,
    Mode::Bot(b) => b.delete(&api.ctx, post).await,
    Mode::Web => Err(api.needs_account("deleting")),
  }
}

pub async fn react(api: &Api, post: &str, emoji: &str, undo: bool) -> Result<Action> {
  match api.mode()? {
    Mode::User(_) => {
      api
        .as_user(async |c| mt::react(c, post, emoji, undo).await)
        .await
    }
    Mode::Bot(b) => b.react(&api.ctx, post, emoji, undo).await,
    Mode::Web => Err(api.needs_account("reacting")),
  }
}

pub async fn pin(api: &Api, message: &str, undo: bool) -> Result<Action> {
  match api.mode()? {
    Mode::User(_) => api.as_user(async |c| mt::pin(c, message, undo).await).await,
    Mode::Bot(b) => b.pin(&api.ctx, message, undo).await,
    Mode::Web => Err(api.needs_account("pinning")),
  }
}

pub async fn forward(api: &Api, message: &str, to: &str) -> Result<Action> {
  match api.mode()? {
    Mode::User(_) => {
      api
        .as_user(async |c| mt::forward(c, message, to).await)
        .await
    }
    Mode::Bot(b) => b.forward(&api.ctx, message, to).await,
    Mode::Web => Err(api.needs_account("forwarding")),
  }
}

/// Media that need the route to fetch them: bot files, or MTProto media.
pub async fn save_media(
  api: &Api,
  media: &Media,
  dir: &Path,
  name: &str,
) -> Option<Result<PathBuf>> {
  if media.url.starts_with("tg-bot-file:") {
    let bot = Bot::from_ctx(&api.ctx)?;
    return Some(bot.save(&api.ctx, media, dir, name).await);
  }
  let post = media.url.strip_prefix("tg-media:")?;
  Some(
    api
      .as_user(async |c| mt::save_media(c, post, dir, name).await)
      .await,
  )
}

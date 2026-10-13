//! Chats: dialogs, members, joining and leaving, unread counts.

use std::collections::BTreeMap;

use atp_core::{Action, Collection, Page, PageReq, Result, User};

use crate::api::{Api, Mode, bots_cannot};
use crate::mt;

pub async fn list(api: &Api, parent: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
  match api.mode()? {
    Mode::User(_) => {
      api
        .as_user(async |c| mt::chats(c, parent, page).await)
        .await
    }
    Mode::Bot(_) => Err(bots_cannot("chats")),
    Mode::Web => Err(api.needs_account("listing chats")),
  }
}

pub async fn members(api: &Api, chat: &str, page: &PageReq) -> Result<Page<User>> {
  match api.mode()? {
    Mode::User(_) => {
      api
        .as_user(async |c| mt::members(c, chat, page).await)
        .await
    }
    Mode::Bot(b) => b.members(&api.ctx, chat).await,
    Mode::Web => Err(api.needs_account("listing members")),
  }
}

/// `follow`: join a chat (or leave it with `undo`).
pub async fn join(api: &Api, chat: &str, undo: bool) -> Result<Action> {
  match api.mode()? {
    Mode::User(_) => api.as_user(async |c| mt::join(c, chat, undo).await).await,
    Mode::Bot(_) => Err(bots_cannot("follow (join)")),
    Mode::Web => Err(api.needs_account("joining")),
  }
}

pub async fn unread(api: &Api) -> Result<BTreeMap<String, u64>> {
  match api.mode()? {
    Mode::User(_) => api.as_user(async |c| mt::unread(c).await).await,
    Mode::Bot(_) => Err(bots_cannot("unread")),
    Mode::Web => Err(api.needs_account("unread")),
  }
}

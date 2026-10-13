//! The bot itself, chats it can see (as users), and their administrators.

use atp_core::{Ctx, Page, Result, User, ValueExt, json};

use super::parse::{chat_user, user};
use super::{Bot, chat_id};
use crate::refs;

impl Bot {
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
}

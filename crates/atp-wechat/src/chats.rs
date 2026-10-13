//! Conversations and their messages, from what this machine recorded
//! ([`crate::archive`]): iLink itself lists nothing.

use std::cmp::Reverse;

use atp_core::{Collection, Error, Page, PageReq, Post, Result, ValueExt};

use crate::api::{Account, Api};
use crate::{archive, parse, refs};

pub fn chats(api: &Api, account: &Account, req: &PageReq) -> Page<Collection> {
  let mut people: Vec<_> = archive::contacts(&api.ctx, account).into_values().collect();
  people.sort_by_key(|c| Reverse(c.last_at));
  let chats: Vec<Collection> = people.into_iter().map(parse::chat).collect();
  archive::page(&chats, req)
}

/// Messages of a chat on record, newest first.
pub fn messages(api: &Api, account: &Account, chat: &str, req: &PageReq) -> Result<Page<Post>> {
  let chat = refs::chat(chat)?;
  let mut posts: Vec<Post> = archive::messages(&api.ctx, account, &chat)
    .iter()
    .map(parse::post)
    .collect();
  posts.reverse();
  Ok(archive::page(&posts, req))
}

pub fn read(api: &Api, account: &Account, post: &str) -> Result<Post> {
  let (chat, id) = refs::message(post)?;
  archive::messages(&api.ctx, account, &chat)
    .iter()
    .find(|m| [m.str("message_id"), m.str("client_id")].contains(&Some(id.clone())))
    .map(parse::post)
    .ok_or_else(|| {
      Error::not_found(format!("message {post} is not on record here")).with_hint(
        "WeChat keeps no history for the bot: only what `listen` / `sync` received is known",
      )
    })
}

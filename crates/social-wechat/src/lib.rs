//! Personal WeChat (微信) for social-cli, through WeChat's own ClawBot bot
//! (the iLink bot API, [`api`]).
//!
//! Scanning a QR code with WeChat ([`login`]) attaches a bot to the account:
//! people (the owner first) message it and it answers. There is no history,
//! contact list or group access: messages arrive once through long polling
//! ([`live`]), and replies need the `context_token` of the conversation.
//! So everything received or sent is recorded on this machine ([`local`]);
//! `chats`, `messages`, `read` and `download` work from that record.
//! Chats are the other person's id; messages are `CHAT/MESSAGE`.

mod api;
mod convert;
mod crypto;
mod extra;
mod live;
mod local;
mod login;
mod media;
mod refs;
mod send;

use std::path::{Path, PathBuf};
use std::time::Duration;

use social_core::output::Stream;
use social_core::text::from_millis;
use social_core::{
  Action, Cap, Choices, Collection, Ctx, Data, Error, Media, Message, Page, PageReq, Platform,
  PlatformInfo, Post, QrStatus, QrTicket, Result, User, ValueExt,
};

use crate::api::Account;

pub struct Wechat {
  ctx: Ctx,
  login: login::Login,
}

impl Wechat {
  fn account(&self) -> Result<Account> {
    Account::require(&self.ctx)
  }
}

/// A page of a list kept in memory: the cursor is the offset.
fn slice<T: Clone>(items: &[T], page: &PageReq) -> Page<T> {
  let from = page.number_or(0) as usize;
  let size = page.size_within(100);
  let end = (from + size).min(items.len());
  let next = (end < items.len()).then(|| end.to_string());
  Page::new(items.get(from..end).unwrap_or_default().to_vec(), next)
}

impl Platform for Wechat {
  const INFO: PlatformInfo = PlatformInfo {
    id: "wechat",
    name: "WeChat",
    aliases: &["weixin", "wx"],
    about: "WeChat (微信) through its official ClawBot: receive and send messages as a bot on your account",
    home: "https://weixin.qq.com",
    cookie_domains: &[],
    required_cookies: &[],
    caps: &[
      Cap::QrLogin,
      Cap::Read,
      Cap::Download,
      Cap::Chats,
      Cap::Messages,
      Cap::Send,
      Cap::Listen,
    ],
    choices: Choices::NONE,
    min_interval: Duration::from_millis(300),
    guide: include_str!("../GUIDE.md"),
  };

  type Extra = extra::Command;

  fn new(ctx: Ctx) -> Result<Self> {
    Ok(Self {
      ctx,
      login: login::Login::default(),
    })
  }

  fn ctx(&self) -> &Ctx {
    &self.ctx
  }

  fn logged_in(&self) -> bool {
    Account::from_ctx(&self.ctx).is_some()
  }

  /// The bot, checked with a short poll (an expired login answers -14);
  /// messages it brings are recorded like any other.
  async fn whoami(&self) -> Result<User> {
    let acct = self.account()?;
    live::fetch(&self.ctx, &acct, Duration::from_secs(2)).await?;
    let mut user = User {
      id: acct.bot_id.clone(),
      name: "ClawBot".into(),
      ..User::default()
    };
    user.extra.insert("owner".into(), acct.owner.into());
    user.extra.insert("api".into(), acct.base.into());
    Ok(user)
  }

  async fn qr_start(&self) -> Result<QrTicket> {
    login::start(&self.ctx, &self.login).await
  }

  async fn qr_poll(&self, _ticket: &QrTicket) -> Result<QrStatus> {
    login::poll(&self.ctx, &self.login).await
  }

  async fn chats(&self, parent: Option<&str>, page: &PageReq) -> Result<Page<Collection>> {
    if parent.is_some() {
      return Err(Error::input("WeChat chats have no channels inside them"));
    }
    let acct = self.account()?;
    let mut people: Vec<_> = local::contacts(&self.ctx, &acct).into_values().collect();
    people.sort_by_key(|c| std::cmp::Reverse(c.last_at));
    let chats: Vec<Collection> = people
      .into_iter()
      .map(|c| {
        let mut chat = Collection {
          id: c.id.clone(),
          kind: "dm".into(),
          name: c.id,
          description: Some(c.last_text).filter(|t| !t.is_empty()),
          items: Some(c.messages),
          ..Collection::default()
        };
        if let Some(at) = from_millis(c.last_at) {
          chat.extra.insert("last_at".into(), at.to_string().into());
        }
        chat
          .extra
          .insert("can_reply".into(), c.context_token.is_some().into());
        chat
      })
      .collect();
    Ok(slice(&chats, page))
  }

  async fn messages(&self, chat: &str, page: &PageReq) -> Result<Page<Post>> {
    let acct = self.account()?;
    let chat = refs::chat(chat)?;
    let mut posts: Vec<Post> = local::messages(&self.ctx, &acct, &chat)
      .iter()
      .map(convert::post)
      .collect();
    posts.reverse();
    Ok(slice(&posts, page))
  }

  async fn read(&self, post: &str) -> Result<Post> {
    let acct = self.account()?;
    let (chat, id) = refs::message(post)?;
    local::messages(&self.ctx, &acct, &chat)
      .iter()
      .find(|m| [m.str("message_id"), m.str("client_id")].contains(&Some(id.clone())))
      .map(convert::post)
      .ok_or_else(|| {
        Error::not_found(format!("message {post} is not on record here")).with_hint(
          "WeChat keeps no history for the bot: only what `listen` / `sync` received is known",
        )
      })
  }

  async fn send(&self, chat: &str, message: &Message) -> Result<Action> {
    let acct = self.account()?;
    let chat = refs::chat(chat)?;
    send::send(&self.ctx, &acct, &chat, message).await
  }

  async fn listen(&self, chats: &[String], out: &Stream) -> Result<()> {
    let acct = self.account()?;
    let chats = chats
      .iter()
      .map(|c| refs::chat(c))
      .collect::<Result<Vec<_>>>()?;
    live::listen(&self.ctx, &acct, &chats, out).await
  }

  async fn save_media(&self, media: &Media, dir: &Path, name: &str) -> Option<Result<PathBuf>> {
    if !media.url.starts_with(convert::SCHEME) {
      return None;
    }
    let acct = match self.account() {
      Ok(a) => a,
      Err(e) => return Some(Err(e)),
    };
    Some(media::save(&self.ctx, &acct, media, dir, name).await)
  }

  async fn run_extra(&self, command: extra::Command) -> Result<Data> {
    extra::run(&self.ctx, command).await
  }
}

//! WeChat-only commands.

use std::time::Duration;

use social_core::{Action, Data, Error, Page, Result, ValueExt, json};

use crate::api::{self, Account};
use crate::{convert, live, local, refs};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Fetch new messages once and record them (what `listen` would print)
  #[command(after_long_help = EX_SYNC)]
  Sync {
    /// Seconds to wait when nothing is queued
    #[arg(long, default_value_t = 3)]
    wait: u64,
  },
  /// Show "typing…" to someone (or stop it with --stop)
  Typing {
    chat: String,
    #[arg(long)]
    stop: bool,
  },
}

const EX_SYNC: &str = "\
Examples:
  social wechat sync                 # new messages since the last sync / listen
  social wechat sync --wait 30 -f jsonl

Messages are only ever delivered once; `chats` and `messages` show what
earlier runs recorded on this machine.";

pub async fn run(ctx: &social_core::Ctx, command: Command) -> Result<Data> {
  let acct = Account::require(ctx)?;
  Ok(match command {
    Command::Sync { wait } => {
      let (msgs, _) = live::fetch(ctx, &acct, Duration::from_secs(wait.max(1))).await?;
      let mut posts: Vec<_> = msgs.iter().map(convert::post).collect();
      posts.reverse();
      Data::Posts(Page::last(posts))
    }
    Command::Typing { chat, stop } => {
      let chat = refs::chat(&ctx.collection_ref(&chat)?)?;
      let token = local::contact(ctx, &acct, &chat).and_then(|c| c.context_token);
      let mut body = json!({ "ilink_user_id": chat });
      if let Some(t) = &token {
        body["context_token"] = t.clone().into();
      }
      let config = api::call(ctx, &acct, "ilink/bot/getconfig", body).await?;
      let ticket = config.str("typing_ticket").ok_or_else(|| {
        Error::upstream("WeChat gave no typing ticket for this chat")
          .with_hint("typing works in chats that messaged the bot recently")
      })?;
      let status = if stop { 2 } else { 1 };
      let body = json!({ "ilink_user_id": chat, "typing_ticket": ticket, "status": status });
      api::call(ctx, &acct, "ilink/bot/sendtyping", body).await?;
      Data::Action(Action::done(
        if stop { "stop-typing" } else { "typing" },
        chat,
      ))
    }
  })
}

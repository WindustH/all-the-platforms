//! WeChat-only commands.

use std::time::Duration;

use atp_core::{Data, Result};

use crate::api::Api;
use crate::{live, messages, refs};

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
    /// Chat: the person's id (`…@im.wechat`) or #N
    chat: String,
    /// Stop showing it
    #[arg(long)]
    stop: bool,
  },
}

const EX_SYNC: &str = "\
Examples:
  atp wechat sync                 # new messages since the last sync / listen
  atp wechat sync --wait 30 -f jsonl

Messages are only ever delivered once; `chats` and `messages` show what
earlier runs recorded on this machine.";

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  let account = api.require_account()?;
  Ok(match command {
    Command::Sync { wait } => {
      let wait = Duration::from_secs(wait.max(1));
      Data::Posts(live::sync(api, &account, wait).await?)
    }
    Command::Typing { chat, stop } => {
      let chat = refs::chat(&api.ctx.collection_ref(&chat)?)?;
      Data::Action(messages::typing(api, &account, &chat, stop).await?)
    }
  })
}

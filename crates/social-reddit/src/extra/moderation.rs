//! Post flags, moderator actions and blocking.

use social_core::{Data, Result};

use crate::api::Api;
use crate::moderation::{self, Flag};
use crate::users;

#[derive(Debug, clap::Subcommand)]
pub enum ModerationCommand {
  /// Turn a flag of your post or comment on (or off with --undo)
  Flag {
    thing: String,
    #[arg(value_enum)]
    flag: Flag,
    #[arg(long)]
    undo: bool,
  },
  /// Approve a post or comment (moderators)
  Approve { thing: String },
  /// Remove a post or comment (moderators)
  Remove {
    thing: String,
    /// Also mark it as spam
    #[arg(long)]
    spam: bool,
  },
  /// Block a user (or unblock with --undo)
  Block {
    user: String,
    #[arg(long)]
    undo: bool,
  },
}

pub(super) async fn run(api: &Api, command: ModerationCommand) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    ModerationCommand::Flag { thing, flag, undo } => {
      Data::Action(moderation::flag(api, &ctx.post_ref(&thing)?, flag, undo).await?)
    }
    ModerationCommand::Approve { thing } => {
      Data::Action(moderation::approve(api, &ctx.post_ref(&thing)?).await?)
    }
    ModerationCommand::Remove { thing, spam } => {
      Data::Action(moderation::remove(api, &ctx.post_ref(&thing)?, spam).await?)
    }
    ModerationCommand::Block { user, undo } => {
      Data::Action(users::block(api, &ctx.user_ref(&user)?, undo).await?)
    }
  })
}

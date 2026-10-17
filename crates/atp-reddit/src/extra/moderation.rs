//! Post flags and moderator actions. Blocking is the shared `block`.

use atp_core::{Data, Result};

use crate::api::Api;
use crate::moderation::{self, Flag};

#[derive(Debug, clap::Subcommand)]
pub enum ModerationCommand {
  /// Turn a flag of your post or comment on (or off with --undo)
  #[command(after_long_help = EX_FLAG)]
  Flag {
    /// Post or comment: id, `t3_…` / `t1_…`, link or #N
    thing: String,
    /// Which flag
    #[arg(value_enum)]
    flag: Flag,
    /// Turn it off instead
    #[arg(long)]
    undo: bool,
  },
  /// Approve a post or comment (moderators)
  Approve {
    /// Post or comment: id, `t3_…` / `t1_…`, link or #N
    thing: String,
  },
  /// Remove a post or comment (moderators)
  Remove {
    /// Post or comment: id, `t3_…` / `t1_…`, link or #N
    thing: String,
    /// Also mark it as spam
    #[arg(long)]
    spam: bool,
  },
}

const EX_FLAG: &str = "\
Examples:
  atp reddit flag 1wt65ox spoiler
  atp reddit flag t1_kx2a9bc replies --undo
  atp reddit flag 1wt65ox sticky

More: atp guide reddit flag";

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
  })
}

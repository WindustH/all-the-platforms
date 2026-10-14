//! Downvotes, a user's comments and the inbox's read state.

use atp_core::cli::PageArgs;
use atp_core::{Data, Result};

use crate::api::Api;
use crate::{notifications, users, write};

#[derive(Debug, clap::Subcommand)]
pub enum ActivityCommand {
  /// Downvote a post or comment (`t1_…` id or comment link)
  Downvote {
    /// Post or comment: id, `t3_…` / `t1_…`, link or #N
    post: String,
    /// Clear the vote instead
    #[arg(long)]
    undo: bool,
  },
  /// Comments written by a user
  UserComments {
    /// User: name, `u/name` or profile link
    user: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Mark inbox items read (all of them when no id is given)
  #[command(after_long_help = EX_MARK_READ)]
  MarkRead {
    /// `t1_…` / `t4_…` ids from `notifications`
    ids: Vec<String>,
    /// Mark them unread instead
    #[arg(long, requires = "ids")]
    unread: bool,
  },
}

const EX_MARK_READ: &str = "\
Examples:
  atp reddit mark-read
  atp reddit mark-read t1_kx2a9bc t4_2b7fz1
  atp reddit mark-read t4_2b7fz1 --unread

More: atp guide reddit mark-read";

pub(super) async fn run(api: &Api, command: ActivityCommand) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    ActivityCommand::Downvote { post, undo } => {
      let (dir, name) = if undo {
        (0, "unvote")
      } else {
        (-1, "downvote")
      };
      Data::Action(write::vote(api, &ctx.post_ref(&post)?, dir, name).await?)
    }
    ActivityCommand::UserComments { user, page } => {
      let user = ctx.user_ref(&user)?;
      Data::Comments(
        page
          .collect_dated(async |r| users::comments(api, &user, &r).await)
          .await?,
      )
    }
    ActivityCommand::MarkRead { ids, unread } => Data::Action(if ids.is_empty() {
      notifications::mark_read(api).await?
    } else {
      notifications::mark(api, &ids, unread).await?
    }),
  })
}

//! Downvotes, a user's comments and the inbox's read state.

use social_core::cli::PageArgs;
use social_core::{Data, Result};

use crate::api::Api;
use crate::{notifications, users, write};

#[derive(Debug, clap::Subcommand)]
pub enum ActivityCommand {
  /// Downvote a post or comment (`t1_…` id or comment link)
  Downvote {
    post: String,
    /// Clear the vote instead
    #[arg(long)]
    undo: bool,
  },
  /// Comments written by a user
  UserComments {
    user: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Mark inbox items read (all of them when no id is given)
  MarkRead {
    /// `t1_…` / `t4_…` ids from `notifications`
    ids: Vec<String>,
    /// Mark them unread instead
    #[arg(long, requires = "ids")]
    unread: bool,
  },
}

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

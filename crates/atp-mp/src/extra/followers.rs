//! Follower tags and the blacklist (official API, verified accounts).

use atp_core::cli::PageArgs;
use atp_core::{Data, Result};

use crate::api::Api;
use crate::users;

#[derive(Debug, clap::Subcommand)]
pub enum FollowerCommand {
  /// Follower tags (API, verified accounts)
  Tags,
  /// Followers carrying a tag (API, verified accounts)
  Tagged {
    /// Tag: its id, from `tags`, or #N
    tag: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Blocked followers (API, verified accounts)
  Blacklist {
    #[command(flatten)]
    page: PageArgs,
  },
}

pub async fn run(api: &Api, command: FollowerCommand) -> Result<Data> {
  Ok(match command {
    FollowerCommand::Tags => Data::Collections(users::tags(api).await?),
    FollowerCommand::Tagged { tag, page } => {
      let tag = api.ctx.collection_ref(&tag)?;
      Data::Users(
        page
          .collect(async |r| users::tagged(api, &tag, &r).await)
          .await?,
      )
    }
    FollowerCommand::Blacklist { page } => Data::Users(
      page
        .collect(async |r| users::blacklist(api, &r).await)
        .await?,
    ),
  })
}

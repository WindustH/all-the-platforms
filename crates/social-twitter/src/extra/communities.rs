//! Communities: details, posts, members, yours, joining.

use social_core::cli::PageArgs;
use social_core::{Data, Result};

use super::one;
use crate::api::Api;
use crate::communities::{self, Sort};

#[derive(Debug, clap::Subcommand)]
pub enum CommunityCommand {
  /// A community: description, rules, members, your role
  Community { community: String },
  /// Posts of a community
  CommunityPosts {
    community: String,
    #[arg(short, long, value_enum, default_value_t)]
    sort: Sort,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Members of a community (with --moderators, its moderators)
  CommunityMembers {
    community: String,
    #[arg(long)]
    moderators: bool,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Communities you belong to
  Communities,
  /// Join a community (or leave it with --undo)
  Join {
    community: String,
    #[arg(long)]
    undo: bool,
  },
}

pub(super) async fn run(api: &Api, command: CommunityCommand) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    CommunityCommand::Community { community } => {
      one(communities::info(api, &ctx.collection_ref(&community)?).await?)
    }
    CommunityCommand::CommunityPosts {
      community,
      sort,
      page,
    } => {
      let community = ctx.collection_ref(&community)?;
      Data::Posts(
        page
          .collect_dated(async |r| communities::posts(api, &community, sort, &r).await)
          .await?,
      )
    }
    CommunityCommand::CommunityMembers {
      community,
      moderators,
      page,
    } => {
      let community = ctx.collection_ref(&community)?;
      Data::Users(
        page
          .collect(async |r| communities::members(api, &community, moderators, &r).await)
          .await?,
      )
    }
    CommunityCommand::Communities => Data::Collections(communities::mine(api).await?),
    CommunityCommand::Join { community, undo } => {
      Data::Action(communities::join(api, &ctx.collection_ref(&community)?, undo).await?)
    }
  })
}

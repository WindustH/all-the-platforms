//! Bluesky commands beyond the shared set.

use atp_core::cli::PageArgs;
use atp_core::{Data, Result};

use crate::api::Api;
use crate::{blocks, posts, records, search};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Repost a post, or take the repost back with --undo
  Repost {
    /// Post: at:// URI, bsky.app link or #N
    post: String,
    /// Take your repost back
    #[arg(long)]
    undo: bool,
  },
  /// Who reposted a post
  RepostedBy {
    /// Post: at:// URI, bsky.app link or #N
    post: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Mute users (or unmute them with --undo): their posts leave your feeds, only you know
  Mute {
    /// Users: @handles, DIDs, profile links, or #N from the last user list
    #[arg(required = true, value_name = "USER")]
    users: Vec<String>,
    /// Unmute instead
    #[arg(long)]
    undo: bool,
  },
  /// Accounts you mute
  Muted {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Topics trending now; `feedgen '#N'` lists the posts of one
  Trends,
  /// Posts of a custom feed (a feed generator)
  #[command(after_long_help = EX_FEEDGEN)]
  Feedgen {
    /// Feed: at:// URI, bsky.app/profile/…/feed/… link, or #N after `trends`
    feed: String,
    #[command(flatten)]
    page: PageArgs,
  },
}

const EX_FEEDGEN: &str = "\
Examples:
  atp bluesky trends
  atp bluesky feedgen '#1'
  atp bluesky feedgen https://bsky.app/profile/bsky.app/feed/whats-hot -n 50

More: atp guide bluesky feedgen";

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Repost { post, undo } => {
      let post = api.ctx.post_ref(&post)?;
      Data::Action(records::mark(api, &post, true, undo).await?)
    }
    Command::RepostedBy { post, page } => {
      let post = api.ctx.post_ref(&post)?;
      Data::Users(
        page
          .collect(async |req| posts::reposted_by(api, &post, &req).await)
          .await?,
      )
    }
    Command::Mute { users, undo } => {
      let users = users
        .iter()
        .map(|u| api.ctx.user_ref(u))
        .collect::<Result<Vec<_>>>()?;
      Data::actions(blocks::mute(api, &users, undo).await?)
    }
    Command::Muted { page } => Data::Users(
      page
        .collect(async |req| blocks::muted(api, &req).await)
        .await?,
    ),
    Command::Trends => Data::Collections(search::trends(api).await?),
    Command::Feedgen { feed, page } => {
      let feed = api.ctx.collection_ref(&feed)?;
      Data::Posts(
        page
          .collect_dated(async |req| posts::generator(api, &feed, &req).await)
          .await?,
      )
    }
  })
}

//! Kuaishou-only commands.

use atp_core::cli::PageArgs;
use atp_core::{Data, Result};

use crate::api::Api;
use crate::{insights, posts, rooms};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Posts Kuaishou recommends next to one
  Related {
    /// Post: photo id, short-video or share link, v.kuaishou.com link, shared text or #N
    post: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// A live room: title, anchor, audience and stream addresses
  Live {
    /// The anchor's live id or a live.kuaishou.com/u/… link
    room: String,
  },
  /// Live rooms on the live site's home page
  Rooms {
    #[command(flatten)]
    page: PageArgs,
  },
  /// The gift panel of a live room (prices in Kuaishou coins)
  Gifts {
    /// The anchor's live id or a live.kuaishou.com/u/… link
    room: String,
  },
  /// Replays of a user's past live streams
  #[command(after_long_help = EX_REPLAYS)]
  Replays {
    /// User id (3x…), profile link or `me`
    user: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Your works in the creator center, with their review state
  Works {
    #[command(flatten)]
    page: PageArgs,
  },
}

const EX_REPLAYS: &str = "\
Examples:
  atp kuaishou replays me
  atp kuaishou replays 3xabcdefgh12345
  atp kuaishou replays https://www.kuaishou.com/profile/3xabcdefgh12345";

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Related { post, page } => {
      let post = api.ctx.post_ref(&post)?;
      Data::Posts(
        page
          .collect(async |r| posts::related(api, &post, &r).await)
          .await?,
      )
    }
    Command::Live { room } => Data::Post(Box::new(rooms::room(api, &room).await?)),
    Command::Rooms { page } => {
      Data::Posts(page.collect(async |r| rooms::list(api, &r).await).await?)
    }
    Command::Gifts { room } => Data::Value(rooms::gifts(api, &room).await?),
    Command::Replays { user, page } => {
      let user = api.ctx.user_ref(&user)?;
      Data::Posts(
        page
          .collect_dated(async |r| rooms::replays(api, &user, &r).await)
          .await?,
      )
    }
    Command::Works { page } => Data::Posts(
      page
        .collect_dated(async |r| insights::works(api, &r).await)
        .await?,
    ),
  })
}

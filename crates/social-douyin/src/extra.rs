//! Douyin-only commands.

use social_core::cli::PageArgs;
use social_core::{Data, Result};

use crate::api::Api;
use crate::{library, posts, rooms};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Posts of a mix (合集)
  Mix {
    /// Mix id or douyin.com/collection/… link
    mix: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Posts that use a music
  Music {
    /// Music id or douyin.com/music/… link
    music: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Posts Douyin recommends next to one
  Related {
    post: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// A live room: title, anchor, audience and stream addresses
  Live {
    /// web_rid or live.douyin.com/… link
    room: String,
  },
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Mix { mix, page } => Data::Posts(
      page
        .collect(async |r| library::mix(api, &mix, &r).await)
        .await?,
    ),
    Command::Music { music, page } => Data::Posts(
      page
        .collect_dated(async |r| library::music(api, &music, &r).await)
        .await?,
    ),
    Command::Related { post, page } => {
      let post = api.ctx.post_ref(&post)?;
      Data::Posts(
        page
          .collect(async |r| posts::related(api, &post, &r).await)
          .await?,
      )
    }
    Command::Live { room } => Data::Post(Box::new(rooms::room(api, &room).await?)),
  })
}

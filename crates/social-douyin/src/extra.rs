//! Douyin-only commands.

use social_core::cli::PageArgs;
use social_core::{Ctx, Data, Result};

use crate::{aweme, library, live};

#[derive(Debug, clap::Subcommand)]
pub enum Extra {
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

pub async fn run(ctx: &Ctx, command: Extra) -> Result<Data> {
  Ok(match command {
    Extra::Mix { mix, page } => Data::Posts(
      page
        .collect(async |r| library::mix(ctx, &mix, &r).await)
        .await?,
    ),
    Extra::Music { music, page } => Data::Posts(
      page
        .collect_dated(async |r| library::music(ctx, &music, &r).await)
        .await?,
    ),
    Extra::Related { post, page } => {
      let post = ctx.post_ref(&post)?;
      Data::Posts(
        page
          .collect(async |r| aweme::related(ctx, &post, &r).await)
          .await?,
      )
    }
    Extra::Live { room } => Data::Post(Box::new(live::room(ctx, &room).await?)),
  })
}

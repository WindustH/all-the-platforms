//! WeChat Official Account commands beyond the shared set.

use social_core::cli::PageArgs;
use social_core::{Data, Result};

use crate::album;
use crate::client::Client;

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Articles of an album (合集), from its link or `BIZ/ALBUM_ID`
  Album {
    album: String,
    #[command(flatten)]
    page: PageArgs,
  },
}

pub async fn run(c: &Client, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Album { album, page } => {
      let album = c.ctx.collection_ref(&album)?;
      Data::Posts(
        page
          .collect_dated(async |r| Ok(album::page(&c.ctx, &album, &r).await?.1))
          .await?,
      )
    }
  })
}

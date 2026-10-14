//! Podcast commands beyond the shared set.

use atp_core::cli::PageArgs;
use atp_core::{Data, Result};

use crate::api::Api;
use crate::shows;

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// A show's episodes from its feed, the newest first
  #[command(after_long_help = EX_EPISODES)]
  Episodes {
    /// Show: Apple Podcasts link or id, feed URL, or #N
    show: String,
    #[command(flatten)]
    page: PageArgs,
  },
}

const EX_EPISODES: &str = "\
Examples:
  atp podcast search 'hardcore history'
  atp podcast episodes '#1' -n 10
  atp podcast episodes https://feeds.example.com/show.xml --since 30d
  atp podcast download '#2' -o ~/Podcasts

More: atp guide podcast episodes";

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Episodes { show, page } => {
      let show = api.ctx.post_ref(&show)?;
      Data::Posts(
        page
          .collect_dated(async |req| shows::episodes(api, &show, &req).await)
          .await?,
      )
    }
  })
}

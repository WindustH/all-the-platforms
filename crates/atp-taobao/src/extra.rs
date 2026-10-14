//! Taobao-only commands: the search box's suggestions.

use atp_core::cli::PageArgs;
use atp_core::{Data, Result};

use crate::api::Api;
use crate::search;

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Words the search box suggests for a keyword
  Suggest {
    keyword: String,
    #[command(flatten)]
    page: PageArgs,
  },
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Suggest { keyword, page } => Data::Posts(
      page
        .collect(async |r| search::suggest(api, &keyword, &r).await)
        .await?,
    ),
  })
}

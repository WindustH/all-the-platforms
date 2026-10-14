//! JD-only commands.

use atp_core::cli::PageArgs;
use atp_core::{Data, Result};

use crate::api::Api;
use crate::{comments, refs, search};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Words the search box suggests for a keyword
  Suggest {
    keyword: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Searches an item page suggests
  Related {
    /// SKU or item link
    item: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// An item's review summary: review count, good rate and review tags
  Rating {
    /// SKU or item link
    item: String,
  },
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Suggest { keyword, page } => Data::Posts(
      page
        .collect(async |r| search::suggest(api, &keyword, &r).await)
        .await?,
    ),
    Command::Related { item, page } => {
      let item = api.ctx.post_ref(&item)?;
      let sku = refs::sku(api, &item).await?;
      Data::Posts(
        page
          .collect(async |r| search::related(api, &sku, &r).await)
          .await?,
      )
    }
    Command::Rating { item } => {
      let item = api.ctx.post_ref(&item)?;
      Data::Value(comments::rating(api, &item).await?)
    }
  })
}

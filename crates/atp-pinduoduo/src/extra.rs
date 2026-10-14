//! Pinduoduo-only commands.

use atp_core::cli::PageArgs;
use atp_core::{Data, Result};

use crate::api::Api;
use crate::{categories, comments};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Categories: 多多进宝 goods tags (with credentials), else the home page's tabs
  Categories {
    /// Parent category id (DDK tags only)
    parent: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Goods of a category or home tab, by its id from `categories`
  Category {
    id: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// A goods' review summary: rating, tags with counts (needs a login)
  ReviewSummary {
    /// Goods id or goods.html link
    goods: String,
  },
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Categories { parent, page } => Data::Collections(
      page
        .collect(async |r| categories::list(api, parent.as_deref(), &r).await)
        .await?,
    ),
    Command::Category { id, page } => {
      let id = api.ctx.collection_ref(&id)?;
      Data::Posts(
        page
          .collect(async |r| categories::goods(api, &id, &r).await)
          .await?,
      )
    }
    Command::ReviewSummary { goods } => {
      let goods = api.ctx.post_ref(&goods)?;
      Data::Value(comments::summary(api, &goods).await?)
    }
  })
}

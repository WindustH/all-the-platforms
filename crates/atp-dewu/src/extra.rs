//! Dewu-only commands.

use atp_core::cli::PageArgs;
use atp_core::{Data, Result};

use crate::api::Api;
use crate::market::{self, Market};
use crate::products;

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Tabs of the www.dewu.com home page (their ids go to `tab`)
  Tabs {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Products of a home page tab, by its id from `tabs`
  Tab {
    id: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Per-size reference prices of an article number (Open Platform)
  Prices {
    /// Article number (货号), e.g. DD1391-100
    article: String,
    /// Region and country of the prices
    #[arg(long, default_value = "CN")]
    region: String,
    /// Currency of the prices (two-decimal ones: CNY, USD, EUR, HKD …)
    #[arg(long, default_value = "CNY")]
    currency: String,
    /// Consignment (寄售) bids instead of spot (现货) ones
    #[arg(long)]
    consign: bool,
  },
  /// 30-day sales per size of an article number (Open Platform)
  Sales {
    /// Article number (货号), e.g. DD1391-100
    article: String,
  },
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Tabs { page } => Data::Collections(
      page
        .collect(async |r| products::tabs(api, &r).await)
        .await?,
    ),
    Command::Tab { id, page } => {
      let id = api.ctx.collection_ref(&id)?;
      Data::Posts(
        page
          .collect_dated(async |r| products::tab(api, &id, &r).await)
          .await?,
      )
    }
    Command::Prices {
      article,
      region,
      currency,
      consign,
    } => {
      let market = Market {
        region: region.to_uppercase(),
        currency: currency.to_uppercase(),
        consign,
      };
      Data::Post(Box::new(market::prices(api, &article, &market).await?))
    }
    Command::Sales { article } => Data::Post(Box::new(market::sales(api, &article).await?)),
  })
}

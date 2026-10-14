//! Xianyu-only commands: the ratings a user received (信用评价) and the
//! search box's suggestions.

use atp_core::cli::PageArgs;
use atp_core::{Data, Result};

use crate::api::Api;
use crate::{search, users};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Ratings a user received from the people they traded with
  #[command(after_long_help = EX_RATINGS)]
  Ratings {
    /// User id, profile link, `me`, or #N from the last list
    user: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Words the search box suggests for a keyword
  Suggest {
    /// The keyword typed so far
    keyword: String,
    #[command(flatten)]
    page: PageArgs,
  },
}

const EX_RATINGS: &str = "\
Examples:
  atp xianyu ratings me
  atp xianyu ratings 'https://www.goofish.com/personal?userId=2201234567' -n 50

More: atp guide xianyu ratings";

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Ratings { user, page } => {
      let user = api.ctx.user_ref(&user)?;
      let ratings = page.collect_dated(async |r| users::ratings(api, &user, &r).await);
      Data::Comments(ratings.await?)
    }
    Command::Suggest { keyword, page } => Data::Posts(
      page
        .collect(async |r| search::suggest(api, &keyword, &r).await)
        .await?,
    ),
  })
}

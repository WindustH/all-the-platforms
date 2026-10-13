//! Instagram commands beyond the shared set.

use social_core::cli::PageArgs;
use social_core::{Data, Result};

use crate::client::Client;
use crate::read;

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Posts of a hashtag
  Tag {
    tag: String,
    /// Newest instead of top posts
    #[arg(long)]
    recent: bool,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Posts of a place (id or /explore/locations/ link)
  Location {
    location: String,
    #[arg(long)]
    recent: bool,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Current stories of a user
  Stories { user: String },
  /// Story highlights of a user
  Highlights { user: String },
  /// The stories inside one highlight
  Highlight { highlight: String },
  /// Marketing API: ad accounts, campaigns, ad sets, ads, insights reports
  #[command(subcommand)]
  Ads(social_meta::ads::Command),
}

pub async fn run(c: &Client, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Tag { tag, recent, page } => Data::Posts(
      page
        .collect_dated(async |r| read::tag(c, &tag, recent, &r).await)
        .await?,
    ),
    Command::Location {
      location,
      recent,
      page,
    } => {
      let location = c.ctx.collection_ref(&location)?;
      Data::Posts(
        page
          .collect_dated(async |r| read::location(c, &location, recent, &r).await)
          .await?,
      )
    }
    Command::Stories { user } => Data::Posts(read::stories(c, &c.ctx.user_ref(&user)?).await?),
    Command::Highlights { user } => {
      Data::Collections(read::highlights(c, &c.ctx.user_ref(&user)?).await?)
    }
    Command::Highlight { highlight } => {
      let h = c.ctx.collection_ref(&highlight)?;
      let id = if h.starts_with("highlight:") {
        h
      } else {
        format!("highlight:{h}")
      };
      c.require_session("highlights")?;
      Data::Posts(read::reel(c, &id).await?)
    }
    Command::Ads(cmd) => {
      let token = c.token().ok_or_else(|| {
        social_core::Error::auth("no Graph API access token")
          .with_hint("set META_ACCESS_TOKEN or run `social instagram login --token <token>`")
      })?;
      let g = social_meta::Graph::new(&c.ctx, social_meta::graph::FACEBOOK, token);
      social_meta::ads::run(&c.ctx, &g, cmd).await?
    }
  })
}

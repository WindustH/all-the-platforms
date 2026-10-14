//! Instagram commands beyond the shared set.

use atp_core::cli::PageArgs;
use atp_core::{Data, Error, Result};
use atp_meta::Graph;
use atp_meta::graph::FACEBOOK;

use crate::api::Api;
use crate::{stories, topics};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Posts of a hashtag
  Tag {
    /// Hashtag: `#tag` or `tag`
    tag: String,
    /// Newest instead of top posts
    #[arg(long)]
    recent: bool,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Posts of a place (id or /explore/locations/ link)
  Location {
    /// Place: id, /explore/locations/ link or #N
    location: String,
    /// Newest instead of top posts
    #[arg(long)]
    recent: bool,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Current stories of a user
  Stories {
    /// User: name, @name, profile link, numeric id or #N
    user: String,
  },
  /// Story highlights of a user
  #[command(after_long_help = EX_HIGHLIGHTS)]
  Highlights {
    /// User: name, @name, profile link, numeric id or #N
    user: String,
  },
  /// The stories inside one highlight
  Highlight {
    /// Highlight: id or `highlight:ID` from `highlights`, or #N after it
    highlight: String,
  },
  /// Marketing API: ad accounts, campaigns, ad sets, ads, insights reports
  #[command(subcommand)]
  Ads(atp_meta::ads::Command),
}

const EX_HIGHLIGHTS: &str = "\
Examples:
  atp instagram highlights @natgeo
  atp instagram highlight '#2'

More: atp guide instagram highlights";

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Tag { tag, recent, page } => Data::Posts(
      page
        .collect_dated(async |req| topics::tag(api, &tag, recent, &req).await)
        .await?,
    ),
    Command::Location {
      location,
      recent,
      page,
    } => {
      let location = api.ctx.collection_ref(&location)?;
      Data::Posts(
        page
          .collect_dated(async |req| topics::location(api, &location, recent, &req).await)
          .await?,
      )
    }
    Command::Stories { user } => {
      Data::Posts(stories::stories(api, &api.ctx.user_ref(&user)?).await?)
    }
    Command::Highlights { user } => {
      Data::Collections(stories::highlights(api, &api.ctx.user_ref(&user)?).await?)
    }
    Command::Highlight { highlight } => {
      let highlight = api.ctx.collection_ref(&highlight)?;
      Data::Posts(stories::highlight(api, &highlight).await?)
    }
    Command::Ads(command) => atp_meta::ads::run(&api.ctx, &ads_graph(api)?, command).await?,
  })
}

/// The Marketing API: always graph.facebook.com.
fn ads_graph(api: &Api) -> Result<Graph<'_>> {
  let token = api.token().ok_or_else(|| {
    Error::auth("no Graph API access token")
      .with_hint("set META_ACCESS_TOKEN or run `atp instagram login --token <token>`")
  })?;
  Ok(Graph::new(&api.ctx, FACEBOOK, token))
}

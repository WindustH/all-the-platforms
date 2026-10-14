//! Discourse commands beyond the shared set.

use atp_core::cli::PageArgs;
use atp_core::{Data, Result, ValueExt, json};

use crate::api::Api;
use crate::topics;

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// The most active topics of a period
  Top {
    /// The period
    #[arg(long, default_value = "weekly", value_parser = clap::builder::PossibleValuesParser::new(topics::PERIODS))]
    period: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// The forum's categories
  Categories,
  /// The topics of a category
  #[command(after_long_help = EX_CATEGORY)]
  Category {
    /// Category: id, slug, /c/ link or #N after `categories`
    category: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// The topics with a tag
  Tag {
    /// Tag, with or without the `#`
    tag: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// The forum: name, description, counts and staff
  About,
}

const EX_CATEGORY: &str = "\
Examples:
  atp discourse categories
  atp discourse category '#2' -n 50
  DISCOURSE_URL=linux.do atp discourse category develop

More: atp guide discourse category";

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Top { period, page } => Data::Posts(
      page
        .collect_dated(async |req| topics::top(api, &period, &req).await)
        .await?,
    ),
    Command::Categories => Data::Collections(topics::categories(api).await?),
    Command::Category { category, page } => {
      let category = api.ctx.collection_ref(&category)?;
      Data::Posts(
        page
          .collect_dated(async |req| topics::category(api, &category, &req).await)
          .await?,
      )
    }
    Command::Tag { tag, page } => Data::Posts(
      page
        .collect_dated(async |req| topics::tagged(api, &tag, &req).await)
        .await?,
    ),
    Command::About => about(api).await?,
  })
}

/// `/about.json`: the forum's name, description and counts.
async fn about(api: &Api) -> Result<Data> {
  let v = api.get("/about.json", Vec::new()).await?;
  let about = v.at("about");
  let staff = |key: &str| -> Vec<String> {
    about
      .list(key)
      .iter()
      .filter_map(|u| u.str("username"))
      .collect()
  };
  Ok(Data::Value(json!({
    "forum": api.forum(),
    "title": about.str("title"),
    "description": about.str("description"),
    "version": about.str("version"),
    "topics": about.u64("stats.topics_count"),
    "posts": about.u64("stats.posts_count"),
    "users": about.u64("stats.users_count"),
    "active_users_30_days": about.u64("stats.active_users_30_days"),
    "admins": staff("admins"),
    "moderators": staff("moderators"),
  })))
}

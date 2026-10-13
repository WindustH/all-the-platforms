//! Facebook commands beyond the shared set.

use atp_core::cli::PageArgs;
use atp_core::{Data, Page, Result};

use crate::client::Client;
use crate::{pages, read};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Pages your Graph API token manages
  Pages,
  /// Remember the Page to act on by default
  UsePage { page: String },
  /// Posts of a Page through the Graph API (ids usable for comment / delete / insights)
  PagePosts {
    /// Page id or username (default: FACEBOOK_PAGE_ID, `use-page`, or your only Page)
    page: Option<String>,
    #[command(flatten)]
    page_args: PageArgs,
  },
  /// A group: name, privacy, members, description
  Group { group: String },
  /// Posts of a group's discussion
  GroupPosts {
    group: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Friends of a profile, as far as Facebook shows them (logged in)
  Friends { user: Option<String> },
  /// Marketing API: ad accounts, campaigns, ad sets, ads, insights reports
  #[command(subcommand)]
  Ads(atp_meta::ads::Command),
}

pub async fn run(c: &Client, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Pages => Data::Collections(Page::last(pages::pages(c).await?)),
    Command::UsePage { page } => Data::Action(pages::use_page(c, &page).await?),
    Command::PagePosts { page, page_args } => {
      let page = page.as_deref();
      Data::Posts(
        page_args
          .collect_dated(async |r| pages::page_posts(c, page, &r).await)
          .await?,
      )
    }
    Command::Group { group } => {
      let group = c.ctx.collection_ref(&group)?;
      Data::Collections(Page::last(vec![read::group(c, &group).await?]))
    }
    Command::GroupPosts { group, page } => {
      let group = c.ctx.collection_ref(&group)?;
      Data::Posts(
        page
          .collect_dated(async |r| read::group_posts(c, &group, &r).await)
          .await?,
      )
    }
    Command::Friends { user } => {
      let user = user.map(|u| c.ctx.user_ref(&u)).transpose()?;
      Data::Users(read::friends(c, user.as_deref()).await?)
    }
    Command::Ads(cmd) => atp_meta::ads::run(&c.ctx, &c.graph()?, cmd).await?,
  })
}

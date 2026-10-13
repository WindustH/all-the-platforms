//! Lists: their tweets, members and memberships.

use atp_core::cli::PageArgs;
use atp_core::{Data, Result};

use super::one;
use crate::api::Api;
use crate::{lists, posts};

#[derive(Debug, clap::Subcommand)]
pub enum ListCommand {
  /// Latest tweets of a list (id or x.com/i/lists/... URL; see `collections`)
  List {
    list: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// A list's name, description, owner and counts
  ListInfo { list: String },
  /// Members of a list (with --followers, the accounts following it)
  ListMembers {
    list: String,
    #[arg(long)]
    followers: bool,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Lists a user (you, when omitted) is a member of
  ListMemberships {
    user: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
}

pub(super) async fn run(api: &Api, command: ListCommand) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    ListCommand::List { list, page } => {
      let list = ctx.collection_ref(&list)?;
      Data::Posts(
        page
          .collect_dated(async |r| posts::list(api, &list, &r).await)
          .await?,
      )
    }
    ListCommand::ListInfo { list } => one(lists::info(api, &ctx.collection_ref(&list)?).await?),
    ListCommand::ListMembers {
      list,
      followers,
      page,
    } => {
      let list = ctx.collection_ref(&list)?;
      Data::Users(
        page
          .collect(async |r| lists::people(api, &list, followers, &r).await)
          .await?,
      )
    }
    ListCommand::ListMemberships { user, page } => {
      let user = user.map(|u| ctx.user_ref(&u)).transpose()?;
      let user = user.as_deref();
      Data::Collections(
        page
          .collect(async |r| lists::memberships(api, user, &r).await)
          .await?,
      )
    }
  })
}

//! Twitter-only commands: retweets and quotes, profile tabs, lists,
//! communities, Spaces, trends by place, mutes, the pinned tweet, scheduled
//! posts and the list of bookmark folders.

mod account;
mod communities;
mod discover;
mod folders;
mod lists;
mod own_lists;
mod posts;

use atp_core::{Collection, Data, Page, Result};

use crate::api::Api;

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  #[command(flatten)]
  Posts(posts::PostCommand),
  #[command(flatten)]
  Lists(lists::ListCommand),
  #[command(flatten)]
  OwnLists(own_lists::OwnListCommand),
  #[command(flatten)]
  Communities(communities::CommunityCommand),
  #[command(flatten)]
  Discover(discover::DiscoverCommand),
  #[command(flatten)]
  Account(account::AccountCommand),
  #[command(flatten)]
  Folders(folders::FolderCommand),
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  match command {
    Command::Posts(command) => posts::run(api, command).await,
    Command::Lists(command) => lists::run(api, command).await,
    Command::OwnLists(command) => own_lists::run(api, command).await,
    Command::Communities(command) => communities::run(api, command).await,
    Command::Discover(command) => discover::run(api, command).await,
    Command::Account(command) => account::run(api, command).await,
    Command::Folders(command) => folders::run(api, command).await,
  }
}

/// One collection as a list.
pub(super) fn one(c: Collection) -> Data {
  Data::Collections(Page::last(vec![c]))
}

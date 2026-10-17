//! Reddit-only commands: browsing one subreddit, a multireddit or a domain,
//! managing your multireddits, community details (rules, moderators, wiki,
//! flairs), your other lists, downvotes, a user's comments, trophies, post
//! flags and flair, moderator actions and the inbox's read state.

mod activity;
mod browse;
mod community;
mod library;
mod moderation;
mod multis;
mod search;

use atp_core::{Data, Result};

use crate::api::Api;

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  #[command(flatten)]
  Browse(browse::BrowseCommand),
  #[command(flatten)]
  Activity(activity::ActivityCommand),
  #[command(flatten)]
  Search(search::SearchCommand),
  #[command(flatten)]
  Community(community::CommunityCommand),
  #[command(flatten)]
  Multis(multis::MultiCommand),
  #[command(flatten)]
  Library(library::LibraryCommand),
  #[command(flatten)]
  Moderation(moderation::ModerationCommand),
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  match command {
    Command::Browse(command) => browse::run(api, command).await,
    Command::Activity(command) => activity::run(api, command).await,
    Command::Search(command) => search::run(api, command).await,
    Command::Community(command) => community::run(api, command).await,
    Command::Multis(command) => multis::run(api, command).await,
    Command::Library(command) => library::run(api, command).await,
    Command::Moderation(command) => moderation::run(api, command).await,
  }
}

/// Orders of a listing.
pub(super) const SORTS: [&str; 5] = ["hot", "new", "top", "rising", "controversial"];

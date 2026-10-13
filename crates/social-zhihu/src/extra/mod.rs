//! Zhihu-only commands, by area.

mod account;
mod browse;
mod explore;
mod questions;

use std::path::PathBuf;

use social_core::cli::check_images;
use social_core::{Data, Error, Result};

use crate::api::Api;
use crate::{refs, users};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  #[command(flatten)]
  Questions(questions::QuestionCommand),
  #[command(flatten)]
  Browse(browse::BrowseCommand),
  #[command(flatten)]
  Explore(explore::ExploreCommand),
  #[command(flatten)]
  Account(account::AccountCommand),
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  match command {
    Command::Questions(command) => questions::run(api, command).await,
    Command::Browse(command) => browse::run(api, command).await,
    Command::Explore(command) => explore::run(api, command).await,
    Command::Account(command) => account::run(api, command).await,
  }
}

/// Refuse an empty title and images that are not files.
fn title_and_images(title: &str, images: &[PathBuf]) -> Result<()> {
  if title.trim().is_empty() {
    return Err(Error::input("the title is empty"));
  }
  check_images(images)
}

/// A user given as `#N`, token or link; the logged-in account when absent.
async fn owner(api: &Api, user: Option<String>) -> Result<String> {
  let token = match user {
    Some(u) => Some(refs::user(&api.ctx.user_ref(&u)?)?),
    None => None,
  };
  users::owner(api, token.as_deref()).await
}

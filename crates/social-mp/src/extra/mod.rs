//! WeChat Official Account commands beyond the shared set, one module per
//! area, flattened into [`Command`].

mod comments;
mod drafts;
mod followers;
mod material;
mod tools;

use std::io::IsTerminal;

use social_core::cli::PageArgs;
use social_core::{Data, Error, Result};

use self::comments::CommentCommand;
use self::drafts::DraftCommand;
use self::followers::FollowerCommand;
use self::material::MaterialCommand;
use self::tools::ToolCommand;
use crate::albums;
use crate::api::Api;

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Articles of an album (合集), from its link or `BIZ/ALBUM_ID`
  Album {
    album: String,
    #[command(flatten)]
    page: PageArgs,
  },
  #[command(flatten)]
  Draft(DraftCommand),
  #[command(flatten)]
  Comment(CommentCommand),
  #[command(flatten)]
  Material(MaterialCommand),
  #[command(flatten)]
  Follower(FollowerCommand),
  #[command(flatten)]
  Tool(ToolCommand),
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  match command {
    Command::Album { album, page } => album_posts(api, &album, page).await,
    Command::Draft(command) => drafts::run(api, command).await,
    Command::Comment(command) => comments::run(api, command).await,
    Command::Material(command) => material::run(api, command).await,
    Command::Follower(command) => followers::run(api, command).await,
    Command::Tool(command) => tools::run(api, command).await,
  }
}

async fn album_posts(api: &Api, album: &str, page: PageArgs) -> Result<Data> {
  let album = api.ctx.collection_ref(album)?;
  let posts = page
    .collect_dated(async |r| Ok(albums::page(&api.ctx, &album, &r).await?.1))
    .await?;
  Ok(Data::Posts(posts))
}

/// Ask before something irreversible, unless `--yes`.
fn confirm(yes: bool, what: &str) -> Result<()> {
  if yes {
    return Ok(());
  }
  if !std::io::stdin().is_terminal() {
    return Err(Error::input(format!("refusing to {what} without --yes")));
  }
  eprint!("{what}? [y/N] ");
  let mut answer = String::new();
  std::io::stdin().read_line(&mut answer)?;
  if matches!(answer.trim(), "y" | "Y" | "yes") {
    Ok(())
  } else {
    Err(Error::input("cancelled"))
  }
}

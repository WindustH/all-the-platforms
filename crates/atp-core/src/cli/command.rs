//! The shared command set: one enum per area, flattened into a single level
//! of subcommands, and the dispatch of each area to its module.

use std::process::ExitCode;

use clap::Subcommand;

use super::account::AccountCommand;
use super::chat::ChatCommand;
use super::library::LibraryCommand;
use super::media::MediaCommand;
use super::read::ReadCommand;
use super::write::WriteCommand;
use super::{account, chat, library, media, read, write};
use crate::account::Loaded;
use crate::error::Result;
use crate::model::Data;
use crate::output::Format;
use crate::platform::Platform;

/// Commands every platform shares. `POST` accepts an id, a URL or `#N` from the
/// last printed list; `USER` likewise.
#[derive(Debug, Subcommand)]
pub enum CommonCommand {
  #[command(flatten)]
  Account(AccountCommand),
  #[command(flatten)]
  Read(ReadCommand),
  #[command(flatten)]
  Library(LibraryCommand),
  #[command(flatten)]
  Write(WriteCommand),
  #[command(flatten)]
  Media(MediaCommand),
  #[command(flatten)]
  Chat(ChatCommand),
}

/// What a command produced, and whether a changed session is saved after it.
pub(super) struct Outcome {
  pub data: Data,
  pub code: ExitCode,
  pub persist: bool,
}

impl Outcome {
  pub(super) fn saved(data: Data) -> Self {
    Self {
      data,
      code: ExitCode::SUCCESS,
      persist: true,
    }
  }
}

/// How the run was started: the session it loaded and how it prints.
pub(super) struct Run<'a> {
  pub loaded: &'a Loaded,
  pub format: Format,
  pub raw: bool,
}

impl CommonCommand {
  pub(super) async fn run<P: Platform>(self, p: &P, run: &Run<'_>) -> Result<Outcome> {
    let data = match self {
      CommonCommand::Account(cmd) => return account::run(p, cmd, run.loaded).await,
      CommonCommand::Read(cmd) => read::run(p, cmd).await?,
      CommonCommand::Library(cmd) => library::run(p, cmd).await?,
      CommonCommand::Write(cmd) => write::run(p, cmd).await?,
      CommonCommand::Media(cmd) => media::run(p, cmd).await?,
      CommonCommand::Chat(cmd) => chat::run(p, cmd, run.format, run.raw).await?,
    };
    Ok(Outcome::saved(data))
  }
}

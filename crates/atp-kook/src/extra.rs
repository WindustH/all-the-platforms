//! KOOK-only commands.

use std::path::PathBuf;

use atp_core::cli::check_files;
use atp_core::{Data, Result};

use crate::api::Api;
use crate::{chats, uploads};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// A server's details
  Server {
    /// Server: `guild:<id>` or #N
    server: String,
  },
  /// A server's roles
  Roles {
    /// Server: `guild:<id>` or #N
    server: String,
  },
  /// Upload a file and print its KOOK asset URL
  Upload {
    /// File to upload
    file: PathBuf,
  },
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    Command::Server { server } => {
      Data::Value(chats::server(api, &ctx.collection_ref(&server)?).await?)
    }
    Command::Roles { server } => {
      Data::Value(chats::roles(api, &ctx.collection_ref(&server)?).await?)
    }
    Command::Upload { file } => {
      check_files(std::slice::from_ref(&file))?;
      Data::Value(uploads::asset(api, &file).await?)
    }
  })
}

//! KOOK-only commands.

use std::path::PathBuf;

use social_core::{Data, Error, Result};

use crate::api::Api;
use crate::refs::{self, Chat};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// A server's details
  Server { server: String },
  /// A server's roles
  Roles { server: String },
  /// Upload a file and print its KOOK asset URL
  Upload { file: PathBuf },
}

fn guild(api: &Api, arg: &str) -> Result<String> {
  match refs::chat(&api.ctx.collection_ref(arg)?)? {
    Chat::Guild(g) => Ok(g),
    _ => Err(Error::input("give a server (guild:<id>)")),
  }
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Server { server } => {
      let g = guild(api, &server)?;
      Data::Value(api.get("/guild/view", &[("guild_id", g)]).await?)
    }
    Command::Roles { server } => {
      let g = guild(api, &server)?;
      Data::Value(api.get("/guild-role/list", &[("guild_id", g)]).await?)
    }
    Command::Upload { file } => {
      social_core::cli::check_files(std::slice::from_ref(&file))?;
      let data = tokio::fs::read(&file).await?;
      let name = file
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .to_owned();
      let part = social_core::http::Part::file("file", data, &name, "application/octet-stream");
      Data::Value(api.upload(vec![part]).await?)
    }
  })
}

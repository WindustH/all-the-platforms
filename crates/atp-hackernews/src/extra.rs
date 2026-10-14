//! Hacker News commands beyond the shared set.

use atp_core::cli::PageArgs;
use atp_core::{Data, Result};

use crate::api::Api;
use crate::search;

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Comments a user wrote, newest first
  Threads {
    /// User: name, profile link or #N
    user: String,
    #[command(flatten)]
    page: PageArgs,
  },
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Threads { user, page } => {
      let user = api.ctx.user_ref(&user)?;
      Data::Posts(
        page
          .collect_dated(async |req| search::comments(api, &user, &req).await)
          .await?,
      )
    }
  })
}

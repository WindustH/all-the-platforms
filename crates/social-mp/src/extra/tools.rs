//! Account configuration and API diagnostics.

use social_core::{Data, Result};

use crate::api::Api;
use crate::tools;

#[derive(Debug, clap::Subcommand)]
pub enum ToolCommand {
  /// The custom menu as configured (API)
  Menu,
  /// The auto-reply rules as configured (API)
  Autoreply,
  /// Daily quota of an API path, e.g. /cgi-bin/draft/add (API)
  Quota { path: String },
  /// Look up a failed API request by the rid in its error (API)
  Rid { rid: String },
  /// Accounts this backend session can switch to
  Accounts,
}

pub async fn run(api: &Api, command: ToolCommand) -> Result<Data> {
  Ok(match command {
    ToolCommand::Menu => Data::Value(tools::menu(api).await?),
    ToolCommand::Autoreply => Data::Value(tools::autoreply(api).await?),
    ToolCommand::Quota { path } => Data::Value(tools::quota(api, &path).await?),
    ToolCommand::Rid { rid } => Data::Value(tools::rid(api, &rid).await?),
    ToolCommand::Accounts => Data::Users(tools::accounts(api).await?),
  })
}

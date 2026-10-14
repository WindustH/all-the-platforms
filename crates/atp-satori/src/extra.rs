//! Satori-only commands.

use atp_core::{Data, Error, Result, Value};

use crate::api::Api;
use crate::users;

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// The accounts the server offers (platform, user, status)
  Logins,
  /// Friends of the account
  Friends {
    /// Continue from an earlier `next`
    #[arg(long)]
    cursor: Option<String>,
  },
  /// Call any API method with a JSON body
  #[command(after_long_help = EX_CALL)]
  Call {
    /// `resource.method`, e.g. `guild.get`
    method: String,
    /// Request body as a JSON object
    #[arg(default_value = "{}")]
    body: String,
  },
}

const EX_CALL: &str = "\
Examples:
  atp satori call guild.get '{\"guild_id\": \"123\"}'
  atp satori call guild.member.kick '{\"guild_id\": \"123\", \"user_id\": \"456\"}'

More: atp guide satori call";

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Logins => Data::Value(users::logins(api).await?),
    Command::Friends { cursor } => Data::Value(users::friends(api, cursor).await?),
    Command::Call { method, body } => {
      let body: Value = serde_json::from_str(&body)
        .map_err(|e| Error::input(format!("the body is not JSON: {e}")))?;
      Data::Value(api.call(&method, body).await?)
    }
  })
}

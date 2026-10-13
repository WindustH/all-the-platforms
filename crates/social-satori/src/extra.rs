//! Satori-only commands.

use social_core::{Data, Error, Result, Value, ValueExt, json};

use crate::api::Api;

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
    #[arg(default_value = "{}")]
    body: String,
  },
}

const EX_CALL: &str = "\
Examples:
  social satori call guild.get '{\"guild_id\": \"123\"}'
  social satori call guild.member.kick '{\"guild_id\": \"123\", \"user_id\": \"456\"}'";

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Logins => {
      let server = api.require()?;
      let logins = api.logins(&server).await?;
      Data::Value(Value::from(
        logins
          .iter()
          .map(|l| {
            json!({
              "platform": l.str("platform"),
              "user_id": l.str("user.id"),
              "name": l.str("user.name"),
              "status": l.at("status"),
              "pick": format!("{}:{}", l.str("platform").unwrap_or_default(), l.str("user.id").unwrap_or_default()),
            })
          })
          .collect::<Vec<_>>(),
      ))
    }
    Command::Friends { cursor } => {
      let mut body = json!({});
      if let Some(c) = cursor {
        body["next"] = json!(c);
      }
      Data::Value(api.call("friend.list", body).await?)
    }
    Command::Call { method, body } => {
      let body: Value = serde_json::from_str(&body)
        .map_err(|e| Error::input(format!("the body is not JSON: {e}")))?;
      Data::Value(api.call(&method, body).await?)
    }
  })
}

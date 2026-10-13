//! QQ-bot-only commands.

use atp_core::{Action, Data, Error, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::refs::{self, Chat};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// A server's (频道) details
  Server { server: String },
  /// A server's roles
  Roles { server: String },
  /// Open a 私信 session with a server member; prints its `dm:` chat
  OpenDm {
    /// The member's user id
    user: String,
    /// The server they share with the bot
    server: String,
  },
  /// Pinned messages of a sub-channel
  Pins { channel: String },
  /// Pin a sub-channel message (or unpin it with --undo)
  Pin {
    message: String,
    #[arg(long)]
    undo: bool,
  },
}

fn guild(api: &Api, arg: &str) -> Result<String> {
  match refs::chat(&api.ctx.collection_ref(arg)?)? {
    Chat::Guild(g) => Ok(g),
    _ => Err(Error::input("give a server (guild:<id>)")),
  }
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Server { server } => Data::Value(
      api
        .get(&format!("/guilds/{}", guild(api, &server)?), &[])
        .await?,
    ),
    Command::Roles { server } => Data::Value(
      api
        .get(&format!("/guilds/{}/roles", guild(api, &server)?), &[])
        .await?,
    ),
    Command::OpenDm { user, server } => {
      let g = guild(api, &server)?;
      let v = api
        .post(
          "/users/@me/dms",
          &json!({"recipient_id": user, "source_guild_id": g}),
        )
        .await?;
      let session = v
        .str("guild_id")
        .ok_or_else(|| Error::upstream("QQ returned no 私信 session"))?;
      Data::Action(Action::done("open-dm", user).with_id(format!("dm:{session}")))
    }
    Command::Pins { channel } => {
      let c = match refs::chat(&api.ctx.collection_ref(&channel)?)? {
        Chat::Channel(c) => c,
        _ => return Err(Error::input("give a sub-channel id")),
      };
      let v = api.get(&format!("/channels/{c}/pins"), &[]).await?;
      let ids: Vec<Value> = v
        .list("message_ids")
        .iter()
        .map(|m| json!(format!("{c}/{}", m.as_str().unwrap_or_default())))
        .collect();
      Data::Value(Value::from(ids))
    }
    Command::Pin { message, undo } => {
      let r = refs::message(&api.ctx.post_ref(&message)?)?;
      let Chat::Channel(c) = &r.chat else {
        return Err(Error::input("only sub-channel messages can be pinned"));
      };
      let path = format!("/channels/{c}/pins/{}", r.id);
      if undo {
        api.delete(&path, &[]).await?;
      } else {
        api.put(&path).await?;
      }
      Data::Action(Action::done(if undo { "unpin" } else { "pin" }, message))
    }
  })
}

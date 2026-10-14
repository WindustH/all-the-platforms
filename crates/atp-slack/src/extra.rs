//! Slack-only commands: the workspace, pins, bookmarks, joining and leaving.

use atp_core::{Action, Data, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::{chats, messages};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// The workspace: name, domain, address
  Workspace,
  /// Pinned messages of a conversation
  Pins {
    /// Conversation: id, archive link, `#name`, a user for your direct one, or #N
    chat: String,
  },
  /// Pin a message, or unpin it with --undo
  #[command(after_long_help = EX_PIN)]
  Pin {
    /// Message: link, `CHANNEL/TS` or #N
    message: String,
    /// Unpin it instead
    #[arg(long)]
    undo: bool,
  },
  /// Bookmarks (links) of a conversation
  Bookmarks {
    /// Conversation: id, archive link, `#name`, a user for your direct one, or #N
    chat: String,
  },
  /// Join a public channel
  #[command(after_long_help = EX_JOIN)]
  Join {
    /// Channel: id, archive link, `#name` or #N
    chat: String,
  },
  /// Leave a conversation
  Leave {
    /// Conversation: id, archive link, `#name` or #N
    chat: String,
  },
}

const EX_PIN: &str = "\
Examples:
  atp slack pin https://acme.slack.com/archives/C0123ABCD/p1700000000000100
  atp slack pin C0123ABCD/1700000000.000100 --undo

More: atp guide slack pin";

const EX_JOIN: &str = "\
Examples:
  atp slack join '#general'
  atp slack join C0123ABCD

More: atp guide slack join";

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    Command::Workspace => workspace(api).await?,
    Command::Pins { chat } => Data::Posts(messages::pins(api, &ctx.collection_ref(&chat)?).await?),
    Command::Pin { message, undo } => {
      Data::Action(messages::pin(api, &ctx.post_ref(&message)?, undo).await?)
    }
    Command::Bookmarks { chat } => bookmarks(api, &ctx.collection_ref(&chat)?).await?,
    Command::Join { chat } => membership(api, &ctx.collection_ref(&chat)?, "join").await?,
    Command::Leave { chat } => membership(api, &ctx.collection_ref(&chat)?, "leave").await?,
  })
}

async fn workspace(api: &Api) -> Result<Data> {
  let v = api.call("team.info", Vec::new()).await?;
  let t = v.at("team");
  Ok(Data::Value(json!({
    "id": t.str("id"),
    "name": t.str("name"),
    "domain": t.str("domain"),
    "url": t.str("url"),
    "email_domain": t.str("email_domain").filter(|d| !d.is_empty()),
    "icon": t.str("icon.image_230"),
    "enterprise": t.str("enterprise_name"),
  })))
}

async fn bookmarks(api: &Api, chat: &str) -> Result<Data> {
  let channel = chats::id(api, chat).await?;
  let v = api
    .call("bookmarks.list", vec![("channel_id", channel)])
    .await?;
  let rows: Vec<Value> = v
    .list("bookmarks")
    .iter()
    .map(|b| json!({"title": b.str("title"), "link": b.str("link"), "emoji": b.str("emoji"), "type": b.str("type")}))
    .collect();
  Ok(Data::Value(json!(rows)))
}

/// `join` or `leave` a conversation.
async fn membership(api: &Api, chat: &str, action: &str) -> Result<Data> {
  let channel = chats::id(api, chat).await?;
  let method = format!("conversations.{action}");
  api
    .write(&method, vec![("channel", channel.clone())])
    .await?;
  Ok(Data::Action(Action::done(action, channel)))
}

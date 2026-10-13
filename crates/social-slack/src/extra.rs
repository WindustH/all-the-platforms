//! Slack-only commands: the workspace, pins, bookmarks, joining and leaving.

use social_core::{Action, Data, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::{chats, messages};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// The workspace: name, domain, plan
  Workspace,
  /// Pinned messages of a conversation
  Pins { chat: String },
  /// Pin a message, or unpin it with --undo
  Pin {
    message: String,
    #[arg(long)]
    undo: bool,
  },
  /// Bookmarks (links) of a conversation
  Bookmarks { chat: String },
  /// Join a public channel
  Join { chat: String },
  /// Leave a conversation
  Leave { chat: String },
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    Command::Workspace => {
      let v = api.call("team.info", Vec::new()).await?;
      let t = v.at("team");
      Data::Value(json!({
        "id": t.str("id"),
        "name": t.str("name"),
        "domain": t.str("domain"),
        "url": t.str("url"),
        "email_domain": t.str("email_domain").filter(|d| !d.is_empty()),
        "icon": t.str("icon.image_230"),
        "enterprise": t.str("enterprise_name"),
      }))
    }
    Command::Pins { chat } => Data::Posts(messages::pins(api, &ctx.collection_ref(&chat)?).await?),
    Command::Pin { message, undo } => {
      Data::Action(messages::pin(api, &ctx.post_ref(&message)?, undo).await?)
    }
    Command::Bookmarks { chat } => {
      let channel = chats::id(api, &ctx.collection_ref(&chat)?).await?;
      let v = api
        .call("bookmarks.list", vec![("channel_id", channel)])
        .await?;
      let rows: Vec<Value> = v
        .list("bookmarks")
        .iter()
        .map(|b| json!({"title": b.str("title"), "link": b.str("link"), "emoji": b.str("emoji"), "type": b.str("type")}))
        .collect();
      Data::Value(json!(rows))
    }
    Command::Join { chat } => {
      let channel = chats::id(api, &ctx.collection_ref(&chat)?).await?;
      api
        .write("conversations.join", vec![("channel", channel.clone())])
        .await?;
      Data::Action(Action::done("join", channel))
    }
    Command::Leave { chat } => {
      let channel = chats::id(api, &ctx.collection_ref(&chat)?).await?;
      api
        .write("conversations.leave", vec![("channel", channel.clone())])
        .await?;
      Data::Action(Action::done("leave", channel))
    }
  })
}

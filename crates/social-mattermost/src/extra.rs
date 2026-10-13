//! Mattermost-only commands: teams, one channel, pins, joining and leaving.

use social_core::{Action, Data, Page, Result, json};

use crate::api::Api;
use crate::{chats, parse, posts};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Your teams
  Teams,
  /// About a channel: purpose, header, message count
  Channel { chat: String },
  /// Pinned posts of a channel
  Pins { chat: String },
  /// Pin a post, or unpin it with --undo
  Pin {
    post: String,
    #[arg(long)]
    undo: bool,
  },
  /// Join a channel
  Join { chat: String },
  /// Leave a channel
  Leave { chat: String },
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    Command::Teams => {
      let server = api.server()?;
      let v = api.get("/users/me/teams", &[]).await?;
      Data::Collections(Page::last(
        v.as_array()
          .into_iter()
          .flatten()
          .map(|t| parse::team(t, &server))
          .collect(),
      ))
    }
    Command::Channel { chat } => {
      let id = chats::channel_id(api, &ctx.collection_ref(&chat)?).await?;
      let v = api.get(&format!("/channels/{id}"), &[]).await?;
      let c = chats::render(api, |view| parse::channel(&v, view)).await?;
      Data::Collections(Page::last(vec![c]))
    }
    Command::Pins { chat } => Data::Posts(posts::pins(api, &ctx.collection_ref(&chat)?).await?),
    Command::Pin { post, undo } => {
      Data::Action(posts::pin(api, &ctx.post_ref(&post)?, undo).await?)
    }
    Command::Join { chat } => {
      let id = chats::channel_id(api, &ctx.collection_ref(&chat)?).await?;
      let me = api.me().await?;
      api
        .post(&format!("/channels/{id}/members"), json!({"user_id": me}))
        .await?;
      Data::Action(Action::done("join", id))
    }
    Command::Leave { chat } => {
      let id = chats::channel_id(api, &ctx.collection_ref(&chat)?).await?;
      let me = api.me().await?;
      api.delete(&format!("/channels/{id}/members/{me}")).await?;
      Data::Action(Action::done("leave", id))
    }
  })
}

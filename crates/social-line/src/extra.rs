//! LINE-only commands.

use social_core::{Data, Result};

use crate::api::Api;
use crate::{account, chats, messages, users};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Send an image by its public HTTPS URL
  Image {
    /// User (U…), group (C…), room (R…), `U…,U…` or `all`
    chat: String,
    url: String,
    /// Smaller preview image URL (default: the image itself)
    #[arg(long)]
    preview: Option<String>,
  },
  /// Send a sticker (package and sticker ids from LINE's sticker list)
  Sticker {
    chat: String,
    package: String,
    sticker: String,
  },
  /// A group's name, picture and member count
  Group { group: String },
  /// Leave a group or room
  Leave { chat: String },
  /// Monthly message quota, usage and the webhook URL
  Quota,
  /// Rich menus and the default one
  Richmenus,
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Image { chat, url, preview } => {
      Data::Action(messages::image(api, &chat, &url, preview.as_deref()).await?)
    }
    Command::Sticker {
      chat,
      package,
      sticker,
    } => Data::Action(messages::sticker(api, &chat, &package, &sticker).await?),
    Command::Group { group } => Data::User(Box::new(users::user(api, &group).await?)),
    Command::Leave { chat } => Data::Action(chats::leave(api, &chat).await?),
    Command::Quota => Data::Value(account::quota(api).await?),
    Command::Richmenus => Data::Value(account::rich_menus(api).await?),
  })
}

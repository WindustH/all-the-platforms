//! Matrix commands beyond the shared set.

use atp_core::cli::PageArgs;
use atp_core::{Data, Query, Result};

use crate::api::Api;
use crate::chats;

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// The public room directory (no login needed); with words, a search
  Rooms {
    /// Words to look for in names, topics and aliases
    query: Option<String>,
    /// Another server's directory (e.g. `gitter.im`)
    #[arg(long)]
    server: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Join a room by id, alias or link
  Join { room: String },
  /// Leave a room
  Leave { room: String },
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    Command::Rooms {
      query,
      server,
      page,
    } => {
      let q = query.map(|keyword| Query {
        keyword,
        ..Query::default()
      });
      Data::Collections(
        page
          .collect(async |r| chats::directory(api, q.as_ref(), server.as_deref(), &r).await)
          .await?,
      )
    }
    Command::Join { room } => {
      Data::Action(chats::join(api, &ctx.collection_ref(&room)?, false).await?)
    }
    Command::Leave { room } => {
      Data::Action(chats::join(api, &ctx.collection_ref(&room)?, true).await?)
    }
  })
}

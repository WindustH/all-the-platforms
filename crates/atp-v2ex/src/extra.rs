//! V2EX commands beyond the shared set.

use atp_core::cli::{PageArgs, confirm};
use atp_core::{Data, Page, Result};

use crate::api::Api;
use crate::{refs, topics, users};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// A node: title, description, topic and follower counts
  Node {
    /// Node: name (`python`), /go/ link or #N
    node: String,
  },
  /// Topics of a node, as on its page
  #[command(after_long_help = EX_TOPICS)]
  Topics {
    /// Node: name (`python`), /go/ link or #N
    node: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Every node, the most topics first
  Nodes {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Delete one of your notifications (needs a token)
  DeleteNotification {
    /// Notification id, as `notifications` lists it
    id: String,
    /// Skip the confirmation question (needed in scripts)
    #[arg(short, long)]
    yes: bool,
  },
  /// The token in use: scope, lifetime, when it was last used
  Token,
}

const EX_TOPICS: &str = "\
Examples:
  atp v2ex topics python -n 20
  atp v2ex topics https://www.v2ex.com/go/apple

Without a token only the node's first page comes back.
More: atp guide v2ex topics";

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    Command::Node { node } => {
      let node = users::node(api, &ctx.collection_ref(&node)?).await?;
      Data::Collections(Page::last(vec![node]))
    }
    Command::Topics { node, page } => {
      let node = refs::node(&ctx.collection_ref(&node)?)?;
      Data::Posts(
        page
          .collect_dated(async |req| topics::node_topics(api, &node, &req).await)
          .await?,
      )
    }
    Command::Nodes { page } => Data::Collections(
      page
        .collect(async |req| users::nodes(api, &req).await)
        .await?,
    ),
    Command::DeleteNotification { id, yes } => {
      api.require()?;
      confirm(yes, &format!("delete notification {id}"))?;
      Data::Action(users::delete_notification(api, &id).await?)
    }
    Command::Token => Data::Value(users::token(api).await?),
  })
}

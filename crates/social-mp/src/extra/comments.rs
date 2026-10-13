//! Comments beyond the shared commands.

use social_core::cli::PageArgs;
use social_core::{Data, Result};

use super::confirm;
use crate::api::Api;
use crate::comments;

#[derive(Debug, clap::Subcommand)]
pub enum CommentCommand {
  /// Your articles with their newest comments (backend)
  LatestComments {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Feature a comment (精选), or stop with --undo (API)
  Elect {
    article: String,
    comment: String,
    #[arg(long)]
    undo: bool,
  },
  /// Open the comments of an article, or close them with --close (API)
  OpenComments {
    article: String,
    #[arg(long)]
    close: bool,
  },
  /// Delete your reply to a comment (API)
  DeleteReply {
    article: String,
    comment: String,
    #[arg(short, long)]
    yes: bool,
  },
}

pub async fn run(api: &Api, command: CommentCommand) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    CommentCommand::LatestComments { page } => Data::Posts(
      page
        .collect(async |r| comments::latest(api, &r).await)
        .await?,
    ),
    CommentCommand::Elect {
      article,
      comment,
      undo,
    } => Data::Action(comments::elect(api, &ctx.post_ref(&article)?, &comment, undo).await?),
    CommentCommand::OpenComments { article, close } => {
      Data::Action(comments::switch(api, &ctx.post_ref(&article)?, close).await?)
    }
    CommentCommand::DeleteReply {
      article,
      comment,
      yes,
    } => {
      confirm(yes, &format!("delete the reply to comment {comment}"))?;
      Data::Action(comments::delete_reply(api, &ctx.post_ref(&article)?, &comment).await?)
    }
  })
}

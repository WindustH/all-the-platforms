//! Comments beyond the shared commands.

use atp_core::cli::{PageArgs, confirm};
use atp_core::{Data, Result};

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
  #[command(after_long_help = EX_ELECT)]
  Elect {
    /// Article: its link, an id from `user-posts`, or #N
    article: String,
    /// Comment id, from `comments`
    comment: String,
    /// Stop featuring it
    #[arg(long)]
    undo: bool,
  },
  /// Open the comments of an article, or close them with --close (API)
  OpenComments {
    /// Article: its link, an id from `user-posts`, or #N
    article: String,
    /// Close them instead
    #[arg(long)]
    close: bool,
  },
  /// Delete your reply to a comment (API)
  DeleteReply {
    /// Article: its link, an id from `user-posts`, or #N
    article: String,
    /// The comment you replied to
    comment: String,
    /// Skip the confirmation question (needed in scripts)
    #[arg(short, long)]
    yes: bool,
  },
}

const EX_ELECT: &str = "\
Examples:
  atp mp comments https://mp.weixin.qq.com/s/AbCdEf
  atp mp elect https://mp.weixin.qq.com/s/AbCdEf 12
  atp mp elect https://mp.weixin.qq.com/s/AbCdEf 12 --undo

Needs the official API and a verified account. More: atp guide mp elect";

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

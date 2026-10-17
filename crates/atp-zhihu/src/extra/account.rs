//! Your account: votes, comment likes and the creator center's list of
//! your posts. Blocks and favorites folders are shared commands.

use atp_core::cli::PageArgs;
use atp_core::{Data, Result};

use crate::api::Api;
use crate::refs::Target;
use crate::{comments, insights, posts};

#[derive(Debug, clap::Subcommand)]
pub enum AccountCommand {
  /// Vote on an answer: up, down (反对) or neutral
  #[command(after_long_help = EX_VOTE)]
  Vote {
    /// Answer (or article / pin): link, a:<id>, answer id or #N
    post: String,
    /// The vote; articles and pins take up or neutral only
    #[arg(value_parser = ["up", "down", "neutral"])]
    direction: String,
  },
  /// Like a comment, or take the like back
  LikeComment {
    /// Comment id
    comment: String,
    /// Take the like back instead
    #[arg(long)]
    undo: bool,
  },
  /// Your own posts with their lifetime numbers from the creator center (内容分析)
  Creations {
    /// Content type
    #[arg(short = 't', long = "type", default_value = "answer", value_parser = ["answer", "article", "pin"])]
    kind: String,
    #[command(flatten)]
    page: PageArgs,
  },
}

const EX_VOTE: &str = "\
Examples:
  atp zhihu vote 2104695667 up
  atp zhihu vote https://www.zhihu.com/question/123/answer/456 down
  atp zhihu vote a:2104695667 neutral

More: atp guide zhihu vote";

pub async fn run(api: &Api, command: AccountCommand) -> Result<Data> {
  match command {
    AccountCommand::Vote { post, direction } => {
      let target = Target::parse(&api.ctx.post_ref(&post)?)?;
      Ok(Data::Action(posts::vote(api, &target, &direction).await?))
    }
    AccountCommand::LikeComment { comment, undo } => Ok(Data::Action(
      comments::like_comment(api, comment.trim(), undo).await?,
    )),
    AccountCommand::Creations { kind, page } => {
      let posts = page.collect_dated(async |r| insights::creations(api, &kind, &r).await);
      Ok(Data::Posts(posts.await?))
    }
  }
}

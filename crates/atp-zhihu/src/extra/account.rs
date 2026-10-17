//! Your account: votes, comment likes, the folders that hold a post,
//! following topics, columns and other people's folders, and the creator
//! center's list of your posts. Blocks and favorites folders are shared
//! commands.

use atp_core::cli::PageArgs;
use atp_core::{Data, Result};

use crate::api::Api;
use crate::refs::{self, Target};
use crate::{comments, insights, library, posts, users};

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
  /// Your favorites folders, each with whether it holds a post
  #[command(after_long_help = EX_POST_FOLDERS)]
  PostFolders {
    /// Answer, article, pin or video: link, a:<id>, answer id or #N
    post: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Follow a topic (话题)
  FollowTopic {
    /// Topic id or link, or #N after `search -t topic`
    topic: String,
    /// Unfollow instead
    #[arg(long)]
    undo: bool,
  },
  /// Follow (subscribe to) a column (专栏)
  FollowColumn {
    /// Column id or link (zhuanlan.zhihu.com/<id>), or #N after a column list
    column: String,
    /// Unfollow instead
    #[arg(long)]
    undo: bool,
  },
  /// Follow someone's favorites folder (收藏夹)
  FollowFolder {
    /// Folder id or link, or #N after `collections USER`
    folder: String,
    /// Unfollow instead
    #[arg(long)]
    undo: bool,
  },
}

const EX_VOTE: &str = "\
Examples:
  atp zhihu vote 2104695667 up
  atp zhihu vote https://www.zhihu.com/question/123/answer/456 down
  atp zhihu vote a:2104695667 neutral

More: atp guide zhihu vote";

const EX_POST_FOLDERS: &str = "\
Examples:
  atp zhihu post-folders https://www.zhihu.com/question/123/answer/456
  atp zhihu favorites -n 50
  atp zhihu post-folders '#3' -n 100

More: atp guide zhihu post-folders";

pub async fn run(api: &Api, command: AccountCommand) -> Result<Data> {
  let ctx = &api.ctx;
  match command {
    AccountCommand::Vote { post, direction } => {
      let target = Target::parse(&ctx.post_ref(&post)?)?;
      Ok(Data::Action(posts::vote(api, &target, &direction).await?))
    }
    AccountCommand::LikeComment { comment, undo } => Ok(Data::Action(
      comments::like_comment(api, comment.trim(), undo).await?,
    )),
    AccountCommand::Creations { kind, page } => {
      let posts = page.collect_dated(async |r| insights::creations(api, &kind, &r).await);
      Ok(Data::Posts(posts.await?))
    }
    AccountCommand::PostFolders { post, page } => {
      let target = Target::parse(&ctx.post_ref(&post)?)?;
      let folders = page.collect(async |r| library::holding(api, &target, &r).await);
      Ok(Data::Collections(folders.await?))
    }
    AccountCommand::FollowTopic { topic, undo } => {
      let id = refs::topic_id(&ctx.collection_ref(&topic)?)?;
      Ok(Data::Action(users::follow(api, "topics", &id, undo).await?))
    }
    AccountCommand::FollowColumn { column, undo } => {
      let id = refs::column_id(&ctx.collection_ref(&column)?)?;
      Ok(Data::Action(users::follow_column(api, &id, undo).await?))
    }
    AccountCommand::FollowFolder { folder, undo } => {
      let id = refs::folder_id(&ctx.collection_ref(&folder)?)?;
      Ok(Data::Action(users::follow_folder(api, &id, undo).await?))
    }
  }
}

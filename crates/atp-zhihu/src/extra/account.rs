//! Your account: votes, comment likes, blocks, favorites folders and the
//! creator center's list of your posts.

use atp_core::cli::PageArgs;
use atp_core::{Data, Error, Result};

use super::title_and_images;
use crate::api::Api;
use crate::refs::Target;
use crate::{comments, insights, library, posts, refs, users};

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
  /// Block a user (屏蔽), or unblock with --undo
  Block {
    /// User: url_token, @token or profile link
    user: String,
    /// Unblock instead
    #[arg(long)]
    undo: bool,
  },
  /// Create a favorites folder (收藏夹)
  CreateFolder {
    /// Folder name
    title: String,
    /// Folder description
    #[arg(short, long, default_value = "")]
    description: String,
    /// Only you can see it
    #[arg(long)]
    private: bool,
  },
  /// Delete one of your favorites folders
  DeleteFolder {
    /// Folder id (see `collections`) or #N after it
    folder: String,
    /// Confirm the deletion (required; there is no question)
    #[arg(short, long)]
    yes: bool,
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
    AccountCommand::Block { user, undo } => {
      let token = refs::user(&api.ctx.user_ref(&user)?)?;
      Ok(Data::Action(users::block(api, &token, undo).await?))
    }
    AccountCommand::CreateFolder {
      title,
      description,
      private,
    } => {
      title_and_images(&title, &[])?;
      let done = library::create_folder(api, title.trim(), &description, private);
      Ok(Data::Action(done.await?))
    }
    AccountCommand::DeleteFolder { folder, yes } => {
      if !yes {
        return Err(Error::input(format!(
          "refusing to delete folder {folder} without --yes"
        )));
      }
      let folder = api.ctx.collection_ref(&folder)?;
      Ok(Data::Action(library::delete_folder(api, &folder).await?))
    }
    AccountCommand::Creations { kind, page } => {
      let posts = page.collect_dated(async |r| insights::creations(api, &kind, &r).await);
      Ok(Data::Posts(posts.await?))
    }
  }
}

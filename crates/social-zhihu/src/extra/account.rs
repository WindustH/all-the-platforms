//! Your account: votes, comment likes, blocks, favorites folders and the
//! creator center's list of your posts.

use social_core::cli::PageArgs;
use social_core::{Data, Error, Result};

use super::title_and_images;
use crate::api::Api;
use crate::refs::Target;
use crate::{comments, insights, library, posts, refs, users};

#[derive(Debug, clap::Subcommand)]
pub enum AccountCommand {
  /// Vote on an answer: up, down (反对) or neutral
  Vote {
    post: String,
    #[arg(value_parser = ["up", "down", "neutral"])]
    direction: String,
  },
  /// Like a comment, or take the like back
  LikeComment {
    /// Comment id
    comment: String,
    #[arg(long)]
    undo: bool,
  },
  /// Block a user (屏蔽), or unblock with --undo
  Block {
    user: String,
    #[arg(long)]
    undo: bool,
  },
  /// Create a favorites folder (收藏夹)
  CreateFolder {
    title: String,
    #[arg(short, long, default_value = "")]
    description: String,
    /// Only you can see it
    #[arg(long)]
    private: bool,
  },
  /// Delete one of your favorites folders
  DeleteFolder {
    /// Folder id (see `collections`)
    folder: String,
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

//! Writing commands: likes, favorites, comments, follows, posts, deletes.

use std::path::PathBuf;

use clap::Subcommand;

use super::input::{check_images, confirm, read_text};
use crate::error::{Error, Result};
use crate::model::Data;
use crate::platform::{Cap, Draft, Platform, Reply};

#[derive(Debug, clap::Args)]
pub struct PublishArgs {
  /// Text to publish; `-` reads it from stdin
  pub text: Option<String>,
  #[arg(long)]
  pub title: Option<String>,
  /// Attach an image (repeatable)
  #[arg(short = 'i', long = "image", value_name = "PATH")]
  pub images: Vec<PathBuf>,
  /// Publish as a reply to this post
  #[arg(long, value_name = "POST")]
  pub reply_to: Option<String>,
  /// Quote / repost this post
  #[arg(long, value_name = "POST")]
  pub quote: Option<String>,
  /// Attach a topic / hashtag (repeatable)
  #[arg(long = "topic", value_name = "TOPIC")]
  pub topics: Vec<String>,
}

#[derive(Debug, Subcommand)]
pub enum WriteCommand {
  /// Like a post
  Like {
    post: String,
    /// Remove the like instead
    #[arg(long)]
    undo: bool,
  },
  /// Remove a like
  Unlike { post: String },
  /// Save a post to favorites / bookmarks
  Favorite {
    post: String,
    /// Folder / collection id
    #[arg(long)]
    folder: Option<String>,
    /// Remove it instead
    #[arg(long)]
    undo: bool,
  },
  /// Remove a post from favorites / bookmarks
  Unfavorite {
    post: String,
    #[arg(long)]
    folder: Option<String>,
  },
  /// Comment on a post, or reply to one of its comments
  #[command(after_long_help = EX_COMMENT)]
  Comment {
    post: String,
    /// Comment text; `-` reads it from stdin
    #[arg(required_unless_present = "images")]
    text: Option<String>,
    /// Reply to this comment id
    #[arg(long, value_name = "COMMENT")]
    reply_to: Option<String>,
    /// Attach an image (repeatable)
    #[arg(short = 'i', long = "image", value_name = "PATH")]
    images: Vec<PathBuf>,
  },
  /// Delete one of your comments
  DeleteComment {
    post: String,
    comment: String,
    #[arg(short, long)]
    yes: bool,
  },
  /// Follow a user
  Follow {
    user: String,
    #[arg(long)]
    undo: bool,
  },
  /// Unfollow a user
  Unfollow { user: String },
  /// Publish a post
  #[command(name = "post", after_long_help = EX_POST)]
  Publish(PublishArgs),
  /// Delete one of your posts
  Delete {
    post: String,
    #[arg(short, long)]
    yes: bool,
  },
}

pub(super) async fn run<P: Platform>(p: &P, cmd: WriteCommand) -> Result<Data> {
  let ctx = p.ctx();
  let action = match cmd {
    WriteCommand::Like { post, undo } => p.like(&ctx.post_ref(&post)?, undo).await?,
    WriteCommand::Unlike { post } => p.like(&ctx.post_ref(&post)?, true).await?,
    WriteCommand::Favorite { post, folder, undo } => {
      let folder = folder.map(|f| ctx.collection_ref(&f)).transpose()?;
      p.favorite(&ctx.post_ref(&post)?, folder.as_deref(), undo)
        .await?
    }
    WriteCommand::Unfavorite { post, folder } => {
      let folder = folder.map(|f| ctx.collection_ref(&f)).transpose()?;
      p.favorite(&ctx.post_ref(&post)?, folder.as_deref(), true)
        .await?
    }
    WriteCommand::Comment {
      post,
      text,
      reply_to,
      images,
    } => comment(p, &post, text, reply_to, images).await?,
    WriteCommand::DeleteComment { post, comment, yes } => {
      confirm(yes, &format!("delete comment {comment}"))?;
      p.delete_comment(&ctx.post_ref(&post)?, &comment).await?
    }
    WriteCommand::Follow { user, undo } => p.follow(&ctx.user_ref(&user)?, undo).await?,
    WriteCommand::Unfollow { user } => p.follow(&ctx.user_ref(&user)?, true).await?,
    WriteCommand::Publish(args) => publish(p, args).await?,
    WriteCommand::Delete { post, yes } => {
      let post = ctx.post_ref(&post)?;
      confirm(yes, &format!("delete {post}"))?;
      p.delete(&post).await?
    }
  };
  Ok(Data::Action(action))
}

async fn comment<P: Platform>(
  p: &P,
  post: &str,
  text: Option<String>,
  reply_to: Option<String>,
  images: Vec<PathBuf>,
) -> Result<crate::model::Action> {
  if !images.is_empty() && !P::INFO.supports(Cap::CommentImages) {
    return Err(Error::unsupported("comment --image"));
  }
  check_images(&images)?;
  let reply = Reply {
    text: read_text(text)?,
    reply_to,
    images,
  };
  if reply.text.is_empty() && reply.images.is_empty() {
    return Err(Error::input("nothing to send: give a text or --image"));
  }
  p.comment(&p.ctx().post_ref(post)?, &reply).await
}

async fn publish<P: Platform>(p: &P, args: PublishArgs) -> Result<crate::model::Action> {
  let ctx = p.ctx();
  let draft = Draft {
    title: args.title,
    text: read_text(args.text)?,
    images: args.images,
    reply_to: args.reply_to.map(|r| ctx.post_ref(&r)).transpose()?,
    quote: args.quote.map(|q| ctx.post_ref(&q)).transpose()?,
    topics: args.topics,
  };
  if draft.text.is_empty() && draft.title.is_none() && draft.images.is_empty() {
    return Err(Error::input(
      "nothing to publish: give a text, --title or --image",
    ));
  }
  check_images(&draft.images)?;
  p.publish(&draft).await
}

const EX_COMMENT: &str = "\
Examples:
  atp bili comment BV1xx411c7mD '好耶'
  atp bili comment '#1' '同意' --reply-to 2384729384
  atp x comment '#1' 'Numbers for this week' -i chart.png

`atp platforms` lists where --image works (comment-images).";

const EX_POST: &str = "\
Examples:
  atp x post \"Hello\" -i photo.jpg
  atp x post \"Nice!\" --reply-to 2104695667180380260
  atp zhihu post \"今天的想法\" -i a.png -i b.png
  atp xhs post \"正文 #话题\" --title 标题 -i cover.jpg
  atp reddit post \"Body text\" --title Title --topic r/test
  cat draft.md | atp bili post -

What `post` creates differs per platform: see `atp guide interact`.";

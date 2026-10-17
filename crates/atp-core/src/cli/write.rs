//! Writing commands: likes, favorites, comments, follows, posts, deletes.

use std::path::PathBuf;

use clap::Subcommand;

use super::input::{check_files, check_images, confirm, read_text};
use crate::error::{Error, Result};
use crate::model::Data;
use crate::platform::{Cap, Draft, Platform, PlatformInfo, Reply};

#[derive(Debug, clap::Args)]
pub struct PublishArgs {
  /// Text to publish; `-` reads it from stdin
  pub text: Option<String>,
  /// Title, where posts have one
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
  #[arg(long = "topic", alias = "tag", value_name = "TOPIC")]
  pub topics: Vec<String>,
  /// Publish a video file
  #[arg(long, value_name = "PATH", conflicts_with_all = ["images", "article"])]
  pub video: Option<PathBuf>,
  /// Publish an article: a Markdown or plain-text file as its body
  #[arg(long, value_name = "PATH", conflicts_with = "images")]
  pub article: Option<PathBuf>,
  /// Cover image of the video or article
  #[arg(long, value_name = "PATH")]
  pub cover: Option<PathBuf>,
  /// Who can see the post
  #[arg(long, value_parser = ["public", "friends", "private"])]
  pub visibility: Option<String>,
  /// Declare the content AI-generated (the platform's AI label)
  #[arg(long)]
  pub ai: bool,
  /// Publish later: +2h, +1d, 2026-10-20 20:00 (local) or an RFC 3339 time
  #[arg(long, value_name = "WHEN", value_parser = crate::text::parse_later)]
  pub schedule: Option<jiff::Timestamp>,
  /// Add the post to one of your collections, by name or id
  #[arg(long, value_name = "COLLECTION")]
  pub collection: Option<String>,
  /// Background music, by the id the platform's music list shows
  #[arg(long, value_name = "MUSIC")]
  pub music: Option<String>,
  /// Skip the confirmation question (needed in scripts)
  #[arg(short, long)]
  pub yes: bool,
}

impl PublishArgs {
  /// The first option given that the platform does not offer.
  fn unsupported(&self, info: &PlatformInfo) -> Option<&'static str> {
    let given = [
      (self.video.is_some(), Cap::PostVideo, "post --video"),
      (self.article.is_some(), Cap::PostArticle, "post --article"),
      (
        self.visibility.is_some(),
        Cap::PostVisibility,
        "post --visibility",
      ),
      (self.ai, Cap::PostAiLabel, "post --ai"),
      (
        self.schedule.is_some(),
        Cap::PostSchedule,
        "post --schedule",
      ),
      (
        self.collection.is_some(),
        Cap::PostCollection,
        "post --collection",
      ),
      (self.music.is_some(), Cap::PostMusic, "post --music"),
    ];
    if let Some((_, _, name)) = given
      .iter()
      .find(|(on, cap, _)| *on && !info.supports(*cap))
    {
      return Some(name);
    }
    let covers = info.supports(Cap::PostVideo) || info.supports(Cap::PostArticle);
    (self.cover.is_some() && !covers).then_some("post --cover")
  }
}

#[derive(Debug, Subcommand)]
pub enum WriteCommand {
  /// Like a post
  Like {
    /// Post: an id, a link, or #N from the last list
    post: String,
    /// Remove the like instead
    #[arg(long)]
    undo: bool,
  },
  /// Remove a like
  Unlike {
    /// Post: an id, a link, or #N from the last list
    post: String,
  },
  /// Save a post to favorites / bookmarks
  Favorite {
    /// Post: an id, a link, or #N from the last list
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
    /// Post: an id, a link, or #N from the last list
    post: String,
    /// Folder / collection id
    #[arg(long)]
    folder: Option<String>,
  },
  /// Comment on a post, or reply to one of its comments
  #[command(after_long_help = EX_COMMENT)]
  Comment {
    /// Post: an id, a link, or #N from the last list
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
    /// Post: an id, a link, or #N from the last list
    post: String,
    /// The comment's id, as `comments` lists it
    comment: String,
    /// Skip the confirmation question (needed in scripts)
    #[arg(short, long)]
    yes: bool,
  },
  /// Follow a user
  Follow {
    /// User: an id, a handle or a link, or #N from the last list
    user: String,
    /// Unfollow instead
    #[arg(long)]
    undo: bool,
  },
  /// Unfollow a user
  Unfollow {
    /// User: an id, a handle or a link, or #N from the last list
    user: String,
  },
  /// Publish a post
  #[command(name = "post", after_long_help = EX_POST)]
  Publish(Box<PublishArgs>),
  /// Delete one of your posts
  Delete {
    /// Post: an id, a link, or #N from the last list
    post: String,
    /// Skip the confirmation question (needed in scripts)
    #[arg(short, long)]
    yes: bool,
  },
}

/// Options of `post` and the capabilities that show them (any of them).
const POST_OPTIONS: &[(&str, &[Cap])] = &[
  ("video", &[Cap::PostVideo]),
  ("article", &[Cap::PostArticle]),
  ("cover", &[Cap::PostVideo, Cap::PostArticle]),
  ("visibility", &[Cap::PostVisibility]),
  ("ai", &[Cap::PostAiLabel]),
  ("schedule", &[Cap::PostSchedule]),
  ("collection", &[Cap::PostCollection]),
  ("music", &[Cap::PostMusic]),
  ("yes", &[Cap::PostVideo, Cap::PostArticle]),
];

/// `post` without the options the platform does not offer.
pub(super) fn post_options(cmd: clap::Command, info: &PlatformInfo) -> clap::Command {
  POST_OPTIONS.iter().fold(cmd, |c, (arg, caps)| {
    if caps.iter().any(|cap| info.supports(*cap)) {
      c
    } else {
      c.mut_arg(*arg, |a| a.hide(true))
    }
  })
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
    WriteCommand::Follow { user, undo } => p.follow(&p.follow_ref(&user)?, undo).await?,
    WriteCommand::Unfollow { user } => p.follow(&p.follow_ref(&user)?, true).await?,
    WriteCommand::Publish(args) => publish(p, *args).await?,
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
  if let Some(option) = args.unsupported(&P::INFO) {
    return Err(Error::unsupported(option));
  }
  let ctx = p.ctx();
  let draft = Draft {
    title: args.title,
    text: read_text(args.text)?,
    images: args.images,
    reply_to: args.reply_to.map(|r| ctx.post_ref(&r)).transpose()?,
    quote: args.quote.map(|q| ctx.post_ref(&q)).transpose()?,
    topics: args.topics,
    video: args.video,
    article: args.article,
    cover: args.cover,
    visibility: args.visibility,
    ai_generated: args.ai,
    schedule: args.schedule,
    collection: args.collection,
    music: args.music,
    yes: args.yes,
  };
  let media = draft.video.is_some() || draft.article.is_some() || !draft.images.is_empty();
  if draft.text.is_empty() && draft.title.is_none() && !media {
    return Err(Error::input(
      "nothing to publish: give a text, --title or --image",
    ));
  }
  check_images(&draft.images)?;
  let files: Vec<PathBuf> = [&draft.video, &draft.article, &draft.cover]
    .into_iter()
    .flatten()
    .cloned()
    .collect();
  check_files(&files)?;
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
  atp douyin post --video clip.mp4 --title 标题 --visibility private
  cat draft.md | atp bili post -

What `post` creates differs per platform: see `atp guide interact`.";

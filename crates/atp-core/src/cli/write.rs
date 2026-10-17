//! Writing commands: likes, favorites, comments, follows, posts, deletes.

use std::path::PathBuf;

use clap::Subcommand;

use super::follows;
use super::input::{check_files, check_images, confirm, read_text};
use crate::error::{Error, Result};
use crate::model::Data;
use crate::platform::{Cap, Draft, Edit, Platform, PlatformInfo, PlatformKind, Reply};

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

#[derive(Debug, clap::Args)]
pub struct EditArgs {
  /// Message or post: `CHAT/MESSAGE`, an id, a link, or #N from the last list
  pub post: String,
  /// New text; `-` reads it from stdin
  pub text: Option<String>,
  /// New title
  #[arg(long)]
  pub title: Option<String>,
  /// Topic / hashtag replacing the post's own (repeatable)
  #[arg(long = "topic", alias = "tag", value_name = "TOPIC")]
  pub topics: Vec<String>,
  /// Remove the post's topics / hashtags
  #[arg(long, conflicts_with = "topics")]
  pub no_topics: bool,
  /// New cover image
  #[arg(long, value_name = "PATH")]
  pub cover: Option<PathBuf>,
  /// Who can see the post
  #[arg(long, value_parser = ["public", "friends", "private"])]
  pub visibility: Option<String>,
  /// Skip the confirmation question (needed in scripts)
  #[arg(short, long)]
  pub yes: bool,
}

impl EditArgs {
  /// The first option given that the platform does not offer.
  fn unsupported(&self, info: &PlatformInfo) -> Option<&'static str> {
    let given = [
      (self.title.is_some(), Cap::EditTitle, "edit --title"),
      (
        !self.topics.is_empty() || self.no_topics,
        Cap::EditTopics,
        "edit --topic",
      ),
      (self.cover.is_some(), Cap::EditCover, "edit --cover"),
      (
        self.visibility.is_some(),
        Cap::EditVisibility,
        "edit --visibility",
      ),
    ];
    given
      .iter()
      .find(|(on, cap, _)| *on && !info.supports(*cap))
      .map(|(_, _, name)| *name)
  }
}

#[derive(Debug, Subcommand)]
pub enum WriteCommand {
  /// Like posts
  #[command(after_long_help = EX_LIKE)]
  Like {
    /// Posts: ids, links, or #N from the last list
    #[arg(required = true, value_name = "POST")]
    posts: Vec<String>,
    /// Remove the likes instead
    #[arg(long)]
    undo: bool,
  },
  /// Remove likes
  Unlike {
    /// Posts: ids, links, or #N from the last list
    #[arg(required = true, value_name = "POST")]
    posts: Vec<String>,
  },
  /// Save posts to favorites / bookmarks
  #[command(after_long_help = EX_FAVORITE)]
  Favorite {
    /// Posts: ids, links, or #N from the last list
    #[arg(required = true, value_name = "POST")]
    posts: Vec<String>,
    /// Folder / collection id
    #[arg(long)]
    folder: Option<String>,
    /// Remove them instead
    #[arg(long)]
    undo: bool,
  },
  /// Remove posts from favorites / bookmarks
  Unfavorite {
    /// Posts: ids, links, or #N from the last list
    #[arg(required = true, value_name = "POST")]
    posts: Vec<String>,
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
  /// Follow users
  #[command(after_long_help = EX_FOLLOW)]
  Follow {
    /// Users: ids, handles or links, or #N from the last list
    #[arg(required = true, value_name = "USER")]
    users: Vec<String>,
    /// Unfollow instead
    #[arg(long)]
    undo: bool,
  },
  /// Unfollow users
  Unfollow {
    /// Users: ids, handles or links, or #N from the last list
    #[arg(required = true, value_name = "USER")]
    users: Vec<String>,
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
  /// Change one of your messages or posts: its text, title, topics, cover or visibility
  #[command(after_long_help = EX_EDIT)]
  Edit(Box<EditArgs>),
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

/// Options of `edit` and the capabilities that show them (any of them).
const EDIT_OPTIONS: &[(&str, &[Cap])] = &[
  ("title", &[Cap::EditTitle]),
  ("topics", &[Cap::EditTopics]),
  ("no_topics", &[Cap::EditTopics]),
  ("cover", &[Cap::EditCover]),
  ("visibility", &[Cap::EditVisibility]),
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

/// `edit` without the options the platform does not offer. `-y` shows
/// where posts are edited (every platform but the chat ones): those edits
/// print the change and ask.
pub(super) fn edit_options(cmd: clap::Command, info: &PlatformInfo) -> clap::Command {
  let cmd = EDIT_OPTIONS.iter().fold(cmd, |c, (arg, caps)| {
    if caps.iter().any(|cap| info.supports(*cap)) {
      c
    } else {
      c.mut_arg(*arg, |a| a.hide(true))
    }
  });
  if info.kind == PlatformKind::Chat {
    cmd.mut_arg("yes", |a| a.hide(true))
  } else {
    cmd
  }
}

pub(super) async fn run<P: Platform>(p: &P, cmd: WriteCommand) -> Result<Data> {
  let ctx = p.ctx();
  let action = match cmd {
    WriteCommand::Like { posts, undo } => return like(p, &posts, undo).await,
    WriteCommand::Unlike { posts } => return like(p, &posts, true).await,
    WriteCommand::Favorite {
      posts,
      folder,
      undo,
    } => return favorite(p, &posts, folder, undo).await,
    WriteCommand::Unfavorite { posts, folder } => {
      return favorite(p, &posts, folder, true).await;
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
    WriteCommand::Follow { users, undo } => return follows::follow(p, &users, undo).await,
    WriteCommand::Unfollow { users } => return follows::follow(p, &users, true).await,
    WriteCommand::Publish(args) => publish(p, *args).await?,
    WriteCommand::Delete { post, yes } => {
      let post = ctx.post_ref(&post)?;
      confirm(yes, &format!("delete {post}"))?;
      p.delete(&post).await?
    }
    WriteCommand::Edit(args) => edit(p, *args).await?,
  };
  Ok(Data::Action(action))
}

/// `like` / `unlike`: one post exactly as before, several through the
/// platform's batch hook.
async fn like<P: Platform>(p: &P, posts: &[String], undo: bool) -> Result<Data> {
  let posts = post_refs(p, posts)?;
  if let [post] = posts.as_slice() {
    return Ok(Data::Action(p.like(post, undo).await?));
  }
  Ok(Data::Actions(p.like_posts(&posts, undo).await?))
}

/// `favorite` / `unfavorite`, as [`like`].
async fn favorite<P: Platform>(
  p: &P,
  posts: &[String],
  folder: Option<String>,
  undo: bool,
) -> Result<Data> {
  let folder = folder.map(|f| p.ctx().collection_ref(&f)).transpose()?;
  let posts = post_refs(p, posts)?;
  if let [post] = posts.as_slice() {
    return Ok(Data::Action(
      p.favorite(post, folder.as_deref(), undo).await?,
    ));
  }
  Ok(Data::Actions(
    p.favorite_posts(&posts, folder.as_deref(), undo).await?,
  ))
}

fn post_refs<P: Platform>(p: &P, list: &[String]) -> Result<Vec<String>> {
  list.iter().map(|post| p.ctx().post_ref(post)).collect()
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

async fn edit<P: Platform>(p: &P, args: EditArgs) -> Result<crate::model::Action> {
  if let Some(option) = args.unsupported(&P::INFO) {
    return Err(Error::unsupported(option));
  }
  let topics = if args.no_topics {
    Some(Vec::new())
  } else {
    Some(args.topics).filter(|t| !t.is_empty())
  };
  let edit = Edit {
    text: args.text.map(|t| read_text(Some(t))).transpose()?,
    title: args.title,
    topics,
    cover: args.cover,
    visibility: args.visibility,
    yes: args.yes,
  };
  let others = edit.title.is_some()
    || edit.topics.is_some()
    || edit.cover.is_some()
    || edit.visibility.is_some();
  if edit.text.is_none() && !others {
    return Err(Error::input(
      "nothing to change: give a new text, --title, --topic, --cover or --visibility",
    ));
  }
  if edit.text_only() == Some("") {
    return Err(Error::input("the new text is empty"));
  }
  if let Some(cover) = &edit.cover {
    check_images(std::slice::from_ref(cover))?;
  }
  p.edit_post(&p.ctx().post_ref(&args.post)?, &edit).await
}

const EX_COMMENT: &str = "\
Examples:
  atp bili comment BV1xx411c7mD '好耶'
  atp bili comment '#1' '同意' --reply-to 2384729384
  atp x comment '#1' 'Numbers for this week' -i chart.png

`atp platforms` lists where --image works (comment-images).";

const EX_LIKE: &str = "\
Examples:
  atp bili like BV1xx411c7mD
  atp douyin likes -n 30
  atp douyin unlike '#1' '#4' '#9'

Several posts give one result each; a failed one does not stop the rest,
unless the platform asks for a login, a check or a pause.
More: atp guide <platform> like";

const EX_FAVORITE: &str = "\
Examples:
  atp bili favorite '#1' --folder 1052622027
  atp douyin favorites -n 50
  atp douyin unfavorite '#2' '#3' '#7'

Several posts give one result each, as with `like`.
More: atp guide <platform> favorite";

const EX_FOLLOW: &str = "\
Examples:
  atp x follow NASA
  atp bili follow 946974 '#2' '#5'
  atp bili unfollow '#3'

Several users give one result each; a failed one does not stop the rest,
unless the platform asks for a login, a check or a pause.
More: atp guide <platform> follow";

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

const EX_EDIT: &str = "\
Examples:
  atp tg edit '#1' 'Fixed the link: https://example.com'
  atp douyin edit 7697611474302225714 --topic 操作系统 --topic 计算机
  atp douyin edit '#2' --title '新标题' --visibility private

Options other than the text show where the platform can change them
(edit-title, edit-topics, edit-cover, edit-visibility in `atp platforms`).
More: atp guide edit";

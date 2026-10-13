//! WeChat Official Account commands beyond the shared set.

use std::io::IsTerminal;
use std::path::PathBuf;

use social_core::cli::{PageArgs, check_images, read_text};
use social_core::{Data, Error, Result};

use crate::client::Client;
use crate::drafts::Compose;
use crate::{album, comments, drafts, material, publish, tools, users};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Articles of an album (合集), from its link or `BIZ/ALBUM_ID`
  Album {
    album: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Your drafts (草稿箱), newest first
  Drafts {
    #[command(flatten)]
    page: PageArgs,
  },
  /// One draft with its text
  Draft { id: String },
  /// How many drafts there are (API)
  DraftCount,
  /// Save a draft with every option (`post` saves one with defaults)
  #[command(after_long_help = EX_DRAFT_NEW)]
  DraftNew {
    /// Title (at most 32 characters through the API)
    #[arg(long)]
    title: String,
    /// Body: HTML when it starts with a tag, else one paragraph per line;
    /// `-` reads it from stdin
    text: Option<String>,
    /// Attach an image at the end (repeatable); the first is the cover
    #[arg(short = 'i', long = "image", value_name = "PATH")]
    images: Vec<PathBuf>,
    /// Cover image (default: the first image)
    #[arg(long, value_name = "PATH")]
    cover: Option<PathBuf>,
    #[arg(long)]
    author: Option<String>,
    /// Summary (at most 120 characters; derived from the body when left out)
    #[arg(long)]
    digest: Option<String>,
    /// The 阅读原文 link
    #[arg(long, value_name = "URL")]
    source_url: Option<String>,
    /// Keep comments closed
    #[arg(long)]
    no_comments: bool,
    /// Only followers may comment
    #[arg(long)]
    fans_only: bool,
  },
  /// Delete a draft
  DraftDelete {
    id: String,
    #[arg(short, long)]
    yes: bool,
  },
  /// Publish a draft (发表); --notify sends it to followers (群发)
  #[command(after_long_help = EX_PUBLISH)]
  Publish {
    draft: String,
    /// Push it to followers as a mass send (群发) instead of only publishing
    #[arg(long)]
    notify: bool,
    #[arg(short, long)]
    yes: bool,
  },
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
  /// The material library (素材)
  Material {
    /// image, video, voice or news
    #[arg(short = 't', long = "type", default_value = "image",
      value_parser = clap::builder::PossibleValuesParser::new(material::KINDS))]
    kind: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// How many materials of each type (API)
  MaterialCount,
  /// Add a permanent material (API)
  MaterialUpload {
    path: PathBuf,
    /// image, thumb, voice or video
    #[arg(short = 't', long = "type", default_value = "image",
      value_parser = clap::builder::PossibleValuesParser::new(["image", "thumb", "voice", "video"]))]
    kind: String,
    /// Video title
    #[arg(long)]
    title: Option<String>,
    /// Video description
    #[arg(long)]
    intro: Option<String>,
  },
  /// Delete a permanent material (API)
  MaterialDelete {
    id: String,
    #[arg(short, long)]
    yes: bool,
  },
  /// Follower tags (API, verified accounts)
  Tags,
  /// Followers carrying a tag (API, verified accounts)
  Tagged {
    tag: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Blocked followers (API, verified accounts)
  Blacklist {
    #[command(flatten)]
    page: PageArgs,
  },
  /// The custom menu as configured (API)
  Menu,
  /// The auto-reply rules as configured (API)
  Autoreply,
  /// Daily quota of an API path, e.g. /cgi-bin/draft/add (API)
  Quota { path: String },
  /// Look up a failed API request by the rid in its error (API)
  Rid { rid: String },
  /// Accounts this backend session can switch to
  Accounts,
}

const EX_DRAFT_NEW: &str = "\
Examples:
  social mp draft-new --title '本周更新' -i cover.jpg '第一段'
  social mp draft-new --title 'Release notes' --author Wish --digest '三个新功能' - < body.html
  social mp drafts

An HTML body may use local images (<img src=\"shot.png\">): they are uploaded.";

const EX_PUBLISH: &str = "\
Examples:
  social mp publish 100000832          # a backend draft: publish without notification
  social mp publish 100000832 --notify # 群发 to followers (once a day for subscriptions)
  social mp publish MEDIA_ID           # an API draft (verified organization accounts)

Backend publishing may show a QR code for the account admin to confirm.";

fn confirm(yes: bool, what: &str) -> Result<()> {
  if yes {
    return Ok(());
  }
  if !std::io::stdin().is_terminal() {
    return Err(Error::input(format!("refusing to {what} without --yes")));
  }
  eprint!("{what}? [y/N] ");
  let mut answer = String::new();
  std::io::stdin().read_line(&mut answer)?;
  if matches!(answer.trim(), "y" | "Y" | "yes") {
    Ok(())
  } else {
    Err(Error::input("cancelled"))
  }
}

pub async fn run(c: &Client, command: Command) -> Result<Data> {
  use Command as C;
  Ok(match command {
    C::Album { album, page } => {
      let album = c.ctx.collection_ref(&album)?;
      Data::Posts(
        page
          .collect_dated(async |r| Ok(album::page(&c.ctx, &album, &r).await?.1))
          .await?,
      )
    }
    C::Drafts { page } => Data::Posts(
      page
        .collect_dated(async |r| drafts::list(c, &r).await)
        .await?,
    ),
    C::Draft { id } => Data::Post(Box::new(drafts::get(c, &c.ctx.post_ref(&id)?).await?)),
    C::DraftCount => Data::Value(social_core::json!({ "drafts": drafts::count(c).await? })),
    C::DraftNew {
      title,
      text,
      images,
      cover,
      author,
      digest,
      source_url,
      no_comments,
      fans_only,
    } => {
      check_images(&images)?;
      check_images(cover.as_slice())?;
      let compose = Compose {
        title,
        text: read_text(text)?,
        images,
        cover,
        author,
        digest,
        source_url,
        no_comments,
        fans_only_comments: fans_only,
      };
      Data::Action(drafts::create(c, &compose).await?)
    }
    C::DraftDelete { id, yes } => {
      let id = c.ctx.post_ref(&id)?;
      confirm(yes, &format!("delete draft {id}"))?;
      Data::Action(drafts::delete(c, &id).await?)
    }
    C::Publish { draft, notify, yes } => {
      let draft = c.ctx.post_ref(&draft)?;
      let what = if notify {
        format!("send draft {draft} to all followers (群发)")
      } else {
        format!("publish draft {draft}")
      };
      confirm(yes, &what)?;
      Data::Action(publish::publish(c, &draft, notify).await?)
    }
    C::LatestComments { page } => Data::Posts(
      page
        .collect(async |r| comments::latest(c, &r).await)
        .await?,
    ),
    C::Elect {
      article,
      comment,
      undo,
    } => Data::Action(comments::elect(c, &c.ctx.post_ref(&article)?, &comment, undo).await?),
    C::OpenComments { article, close } => {
      Data::Action(comments::switch(c, &c.ctx.post_ref(&article)?, close).await?)
    }
    C::DeleteReply {
      article,
      comment,
      yes,
    } => {
      confirm(yes, &format!("delete the reply to comment {comment}"))?;
      Data::Action(comments::delete_reply(c, &c.ctx.post_ref(&article)?, &comment).await?)
    }
    C::Material { kind, page } => Data::Posts(
      page
        .collect_dated(async |r| material::list(c, &kind, &r).await)
        .await?,
    ),
    C::MaterialCount => Data::Value(material::count(c).await?),
    C::MaterialUpload {
      path,
      kind,
      title,
      intro,
    } => Data::Action(material::add(c, &path, &kind, title.as_deref(), intro.as_deref()).await?),
    C::MaterialDelete { id, yes } => {
      confirm(yes, &format!("delete material {id}"))?;
      Data::Action(material::delete(c, &id).await?)
    }
    C::Tags => Data::Collections(users::tags(c).await?),
    C::Tagged { tag, page } => {
      let tag = c.ctx.collection_ref(&tag)?;
      Data::Users(
        page
          .collect(async |r| users::tagged(c, &tag, &r).await)
          .await?,
      )
    }
    C::Blacklist { page } => Data::Users(
      page
        .collect(async |r| users::blacklist(c, &r).await)
        .await?,
    ),
    C::Menu => Data::Value(tools::menu(c).await?),
    C::Autoreply => Data::Value(tools::autoreply(c).await?),
    C::Quota { path } => Data::Value(tools::quota(c, &path).await?),
    C::Rid { rid } => Data::Value(tools::rid(c, &rid).await?),
    C::Accounts => Data::Users(tools::accounts(c).await?),
  })
}

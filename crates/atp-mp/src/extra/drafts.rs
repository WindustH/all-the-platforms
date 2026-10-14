//! Drafts and publishing.

use std::path::PathBuf;

use atp_core::cli::{PageArgs, check_images, read_text};
use atp_core::{Data, Result, json};

use super::confirm;
use crate::api::Api;
use crate::drafts::{self, Compose};
use crate::publish;

#[derive(Debug, clap::Subcommand)]
pub enum DraftCommand {
  /// Your drafts (草稿箱), newest first
  Drafts {
    #[command(flatten)]
    page: PageArgs,
  },
  /// One draft with its text
  Draft {
    /// Draft: a backend number, an API `media_id`, or #N
    id: String,
  },
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
    /// The article's author
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
    /// Draft: a backend number, an API `media_id`, or #N
    id: String,
    /// Skip the confirmation question (needed in scripts)
    #[arg(short, long)]
    yes: bool,
  },
  /// Publish a draft (发表); --notify sends it to followers (群发)
  #[command(after_long_help = EX_PUBLISH)]
  Publish {
    /// Draft: a backend number, an API `media_id`, or #N
    draft: String,
    /// Push it to followers as a mass send (群发) instead of only publishing
    #[arg(long)]
    notify: bool,
    /// Skip the confirmation question (needed in scripts)
    #[arg(short, long)]
    yes: bool,
  },
}

pub async fn run(api: &Api, command: DraftCommand) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    DraftCommand::Drafts { page } => Data::Posts(
      page
        .collect_dated(async |r| drafts::list(api, &r).await)
        .await?,
    ),
    DraftCommand::Draft { id } => {
      Data::Post(Box::new(drafts::get(api, &ctx.post_ref(&id)?).await?))
    }
    DraftCommand::DraftCount => Data::Value(json!({ "drafts": drafts::count(api).await? })),
    DraftCommand::DraftNew {
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
      Data::Action(drafts::create(api, &compose).await?)
    }
    DraftCommand::DraftDelete { id, yes } => {
      let id = ctx.post_ref(&id)?;
      confirm(yes, &format!("delete draft {id}"))?;
      Data::Action(drafts::delete(api, &id).await?)
    }
    DraftCommand::Publish { draft, notify, yes } => {
      let draft = ctx.post_ref(&draft)?;
      let what = if notify {
        format!("send draft {draft} to all followers (群发)")
      } else {
        format!("publish draft {draft}")
      };
      confirm(yes, &what)?;
      Data::Action(publish::publish(api, &draft, notify).await?)
    }
  })
}

const EX_DRAFT_NEW: &str = "\
Examples:
  atp mp draft-new --title '本周更新' -i cover.jpg '第一段'
  atp mp draft-new --title 'Release notes' --author Wish --digest '三个新功能' - < body.html
  atp mp drafts

An HTML body may use local images (<img src=\"shot.png\">): they are uploaded.";

const EX_PUBLISH: &str = "\
Examples:
  atp mp publish 100000832          # a backend draft: publish without notification
  atp mp publish 100000832 --notify # 群发 to followers (once a day for subscriptions)
  atp mp publish MEDIA_ID           # an API draft (verified organization accounts)

Backend publishing may show a QR code for the account admin to confirm.";

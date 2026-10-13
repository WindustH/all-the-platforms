//! Misskey commands beyond the shared set.

use std::path::PathBuf;

use atp_core::cli::{PageArgs, check_images, read_text};
use atp_core::{Action, Data, Result, ValueExt, json};

use crate::api::Api;
use crate::posts::{self, NoteOptions};
use crate::search;

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Renote (boost) a note, or take the renote back with --undo
  Renote {
    note: String,
    #[arg(long)]
    undo: bool,
  },
  /// Write a note with every option Misskey has
  Note {
    /// Text; `-` reads it from stdin
    text: Option<String>,
    /// Content warning shown before the text
    #[arg(long)]
    cw: Option<String>,
    /// Who sees it
    #[arg(long, value_parser = ["public", "home", "followers", "specified"])]
    visibility: Option<String>,
    /// Keep it on this server (no federation)
    #[arg(long)]
    local_only: bool,
    #[arg(long, value_name = "NOTE")]
    reply_to: Option<String>,
    #[arg(long, value_name = "NOTE")]
    quote: Option<String>,
    /// Attach a file (repeatable)
    #[arg(short = 'i', long = "image", value_name = "PATH")]
    files: Vec<PathBuf>,
  },
  /// Hashtags trending on the instance
  Trends,
  /// Notes with a hashtag
  Tag {
    tag: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Notes that mention you
  Mentions {
    #[command(flatten)]
    page: PageArgs,
  },
  /// The instance: name, version, limits and counts
  Instance,
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Renote { note, undo } => {
      Data::Action(posts::renote(api, &api.ctx.post_ref(&note)?, undo).await?)
    }
    Command::Note {
      text,
      cw,
      visibility,
      local_only,
      reply_to,
      quote,
      files,
    } => {
      let options = NoteOptions {
        cw,
        visibility,
        local_only,
        reply_to,
        quote,
        files,
      };
      Data::Action(note(api, text, options).await?)
    }
    Command::Trends => Data::Collections(search::trends(api).await?),
    Command::Tag { tag, page } => Data::Posts(
      page
        .collect_dated(async |req| search::tagged(api, &tag, &req).await)
        .await?,
    ),
    Command::Mentions { page } => Data::Posts(
      page
        .collect_dated(async |req| posts::mentions(api, &req).await)
        .await?,
    ),
    Command::Instance => instance(api).await?,
  })
}

/// `note`: `#N` references resolved, the text read, then [`posts::create`].
async fn note(api: &Api, text: Option<String>, mut options: NoteOptions) -> Result<Action> {
  check_images(&options.files)?;
  let ctx = &api.ctx;
  options.reply_to = options.reply_to.map(|r| ctx.post_ref(&r)).transpose()?;
  options.quote = options.quote.map(|q| ctx.post_ref(&q)).transpose()?;
  posts::create(api, read_text(text)?.trim(), &options).await
}

/// The instance: name, version, limits and counts.
async fn instance(api: &Api) -> Result<Data> {
  let meta = api.call("meta", json!({"detail": true})).await?;
  let stats = api.call("stats", json!({})).await.unwrap_or_default();
  Ok(Data::Value(json!({
    "host": api.instance(),
    "name": meta.str("name"),
    "version": meta.str("version"),
    "description": meta.str("description"),
    "max_note_length": meta.u64("maxNoteTextLength"),
    "registration_open": meta.bool("disableRegistration").map(|disabled| !disabled),
    "users": stats.u64("originalUsersCount"),
    "notes": stats.u64("originalNotesCount"),
    "known_instances": stats.u64("instances"),
  })))
}

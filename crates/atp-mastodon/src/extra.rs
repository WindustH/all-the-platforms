//! Mastodon commands beyond the shared set.

use std::path::PathBuf;

use atp_core::cli::{PageArgs, check_images, read_text};
use atp_core::{Action, Data, Result, ValueExt, json};

use crate::api::Api;
use crate::posts::{self, StatusOptions};
use crate::search;

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Boost (reblog) a status, or take the boost back with --undo
  Boost {
    /// Status: id, link or #N
    status: String,
    /// Take your boost back
    #[arg(long)]
    undo: bool,
  },
  /// Write a status with every option Mastodon has
  #[command(after_long_help = EX_COMPOSE)]
  Compose {
    /// Text; `-` reads it from stdin
    text: Option<String>,
    /// Content warning shown before the text
    #[arg(long)]
    cw: Option<String>,
    /// Who sees it (`direct` is a private mention)
    #[arg(long, value_parser = ["public", "unlisted", "private", "direct"])]
    visibility: Option<String>,
    /// Language of the text, as an ISO 639 code (en, zh, ja)
    #[arg(long)]
    language: Option<String>,
    /// Mark the attached media as sensitive
    #[arg(long)]
    sensitive: bool,
    /// Reply to this status (id, link or #N)
    #[arg(long, value_name = "STATUS")]
    reply_to: Option<String>,
    /// Quote this status (id, link or #N; Mastodon 4.5 and later)
    #[arg(long, value_name = "STATUS")]
    quote: Option<String>,
    /// Attach an image, video or audio file (repeatable, 4 at most)
    #[arg(short = 'i', long = "image", value_name = "PATH")]
    files: Vec<PathBuf>,
  },
  /// Hashtags trending on the instance
  Trends {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Statuses with a hashtag
  Tag {
    /// Hashtag, with or without the `#`
    tag: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// The instance: name, version, limits and rules
  Instance,
}

const EX_COMPOSE: &str = "\
Examples:
  atp mastodon compose 'Spoilers inside' --cw 'Film ending'
  atp mastodon compose '@friend@example.social see you at 8' --visibility direct
  atp mastodon compose 'Same here' --reply-to '#2' -i photo.jpg

More: atp guide mastodon compose";

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Boost { status, undo } => {
      let status = api.ctx.post_ref(&status)?;
      Data::Action(posts::toggle(api, &status, "reblog", "unreblog", undo).await?)
    }
    Command::Compose {
      text,
      cw,
      visibility,
      language,
      sensitive,
      reply_to,
      quote,
      files,
    } => {
      let options = StatusOptions {
        cw,
        visibility,
        language,
        sensitive,
        reply_to,
        quote,
        files,
      };
      Data::Action(compose(api, text, options).await?)
    }
    Command::Trends { page } => Data::Collections(
      page
        .collect(async |req| search::trends(api, &req).await)
        .await?,
    ),
    Command::Tag { tag, page } => Data::Posts(
      page
        .collect_dated(async |req| search::tagged(api, &tag, &req).await)
        .await?,
    ),
    Command::Instance => instance(api).await?,
  })
}

/// `compose`: `#N` references resolved, the text read, then [`posts::create`].
async fn compose(api: &Api, text: Option<String>, mut options: StatusOptions) -> Result<Action> {
  check_images(&options.files)?;
  let ctx = &api.ctx;
  options.reply_to = options.reply_to.map(|r| ctx.post_ref(&r)).transpose()?;
  options.quote = options.quote.map(|q| ctx.post_ref(&q)).transpose()?;
  let text = read_text(text)?;
  if text.trim().is_empty() && options.files.is_empty() {
    return Err(atp_core::Error::input(
      "nothing to post: give a text or -i FILE",
    ));
  }
  posts::create(api, text.trim(), &options).await
}

/// The instance (`/api/v2/instance`): name, version, limits and rules.
async fn instance(api: &Api) -> Result<Data> {
  let v = api.get("v2/instance", Vec::new()).await?;
  let rules: Vec<String> = v
    .list("rules")
    .iter()
    .filter_map(|r| r.str("text"))
    .collect();
  Ok(Data::Value(json!({
    "host": v.str("domain").unwrap_or_else(|| api.instance()),
    "name": v.str("title"),
    "version": v.str("version"),
    "description": v.str("description"),
    "max_characters": v.u64("configuration.statuses.max_characters"),
    "max_media": v.u64("configuration.statuses.max_media_attachments"),
    "registrations_open": v.bool("registrations.enabled"),
    "active_users_month": v.u64("usage.users.active_month"),
    "contact": v.str("contact.email"),
    "rules": rules,
  })))
}

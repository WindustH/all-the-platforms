//! Your account: blocks and mutes, the pinned tweet, scheduled posts.

use std::path::PathBuf;

use social_core::cli::PageArgs;
use social_core::{Data, Error, Result};

use crate::api::Api;
use crate::{account, uploads};

#[derive(Debug, clap::Subcommand)]
pub enum AccountCommand {
  /// Block a user (or unblock with --undo)
  Block {
    user: String,
    #[arg(long)]
    undo: bool,
  },
  /// Mute a user (or unmute with --undo)
  Mute {
    user: String,
    #[arg(long)]
    undo: bool,
  },
  /// Accounts you block
  Blocked {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Accounts you mute
  Muted {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Pin a tweet to your profile (or unpin it with --undo)
  Pin {
    post: String,
    #[arg(long)]
    undo: bool,
  },
  /// Your scheduled posts
  Scheduled,
  /// Schedule a post: --at 2026-10-20T09:30:00+08:00, '2026-10-20 09:30' or +2h
  Schedule {
    text: String,
    #[arg(long, value_name = "WHEN")]
    at: String,
    #[arg(short = 'i', long = "image", value_name = "PATH")]
    images: Vec<PathBuf>,
  },
  /// Cancel a scheduled post
  Unschedule { id: String },
}

pub(super) async fn run(api: &Api, command: AccountCommand) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    AccountCommand::Block { user, undo } => {
      Data::Action(account::block(api, &ctx.user_ref(&user)?, false, undo).await?)
    }
    AccountCommand::Mute { user, undo } => {
      Data::Action(account::block(api, &ctx.user_ref(&user)?, true, undo).await?)
    }
    AccountCommand::Blocked { page } => Data::Users(
      page
        .collect(async |r| account::blocked(api, false, &r).await)
        .await?,
    ),
    AccountCommand::Muted { page } => Data::Users(
      page
        .collect(async |r| account::blocked(api, true, &r).await)
        .await?,
    ),
    AccountCommand::Pin { post, undo } => {
      Data::Action(account::pin(api, &ctx.post_ref(&post)?, undo).await?)
    }
    AccountCommand::Scheduled => Data::Posts(account::scheduled(api).await?),
    AccountCommand::Schedule { text, at, images } => {
      let at = future_time(&at)?;
      social_core::cli::check_images(&images)?;
      let media = uploads::images(api, &images).await?;
      Data::Action(account::schedule(api, &text, at, &media).await?)
    }
    AccountCommand::Unschedule { id } => Data::Action(account::unschedule(api, &id).await?),
  })
}

/// A time to schedule at: `+30m` / `+2h` / `+1d`, a local `YYYY-MM-DD HH:MM`,
/// or RFC 3339. Returns unix seconds, which must lie in the future.
fn future_time(s: &str) -> Result<i64> {
  let s = s.trim();
  let now = jiff::Timestamp::now();
  let bad = || Error::input(format!("bad offset `{s}` (use +30m, +2h or +1d)"));
  let at = if let Some(rel) = s.strip_prefix('+') {
    let (num, unit) = rel.split_at(rel.len().saturating_sub(1));
    let n: i64 = num.parse().map_err(|_| bad())?;
    let secs = match unit {
      "m" => 60,
      "h" => 3600,
      "d" => 86_400,
      _ => return Err(bad()),
    };
    now.as_second() + n * secs
  } else if let Ok(t) = s.parse::<jiff::Timestamp>() {
    t.as_second()
  } else {
    let civil: jiff::civil::DateTime = s.replace(' ', "T").parse().map_err(|_| {
      Error::input(format!(
        "`{s}` is not a time (+2h, '2026-10-20 09:30' or RFC 3339)"
      ))
    })?;
    civil
      .to_zoned(jiff::tz::TimeZone::system())
      .map_err(|e| Error::input(e.to_string()))?
      .timestamp()
      .as_second()
  };
  if at <= now.as_second() {
    return Err(Error::input(format!("{s} is not in the future")));
  }
  Ok(at)
}

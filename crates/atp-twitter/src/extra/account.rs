//! Your account: mutes, follow requests, the pinned tweet, scheduled posts.
//! Blocks are the shared `block` / `unblock` / `blocked`.

use std::path::PathBuf;

use atp_core::cli::PageArgs;
use atp_core::{Data, Error, Result};

use crate::api::Api;
use crate::{account, relations, uploads};

const EX_SCHEDULE: &str = "\
Examples:
  atp x schedule \"Launch day\" --at +2h
  atp x schedule \"Good morning\" --at '2026-10-20 09:30' -i sunrise.jpg
  atp x scheduled                      # ids for unschedule

More: atp guide x schedule";

const EX_FOLLOW_REQUESTS: &str = "\
Examples:
  atp x follow-requests
  atp x accept-follower '#1' '#2'
  atp x deny-follower someone

More: atp guide x follow-requests";

#[derive(Debug, clap::Subcommand)]
pub enum AccountCommand {
  /// Mute a user (or unmute with --undo)
  Mute {
    /// User: handle, @handle, link, id or #N
    user: String,
    /// Unmute instead
    #[arg(long)]
    undo: bool,
  },
  /// Accounts you mute
  Muted {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Accounts asking to follow your protected account
  #[command(after_long_help = EX_FOLLOW_REQUESTS)]
  FollowRequests {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Accept follow requests: let the users follow you
  AcceptFollower {
    /// Users: handles, @handles, links, ids, or #N from the last user list
    #[arg(required = true, value_name = "USER")]
    users: Vec<String>,
  },
  /// Decline follow requests
  DenyFollower {
    /// Users: handles, @handles, links, ids, or #N from the last user list
    #[arg(required = true, value_name = "USER")]
    users: Vec<String>,
  },
  /// Pin a tweet to your profile (or unpin it with --undo)
  Pin {
    /// Tweet: id, link or #N
    post: String,
    /// Unpin instead
    #[arg(long)]
    undo: bool,
  },
  /// Your scheduled posts
  Scheduled,
  /// Schedule a post: --at 2026-10-20T09:30:00+08:00, '2026-10-20 09:30' or +2h
  #[command(after_long_help = EX_SCHEDULE)]
  Schedule {
    /// Text of the post
    text: String,
    /// When: +30m, +2h, +1d, a local '2026-10-20 09:30' or an RFC 3339 time
    #[arg(long, value_name = "WHEN")]
    at: String,
    /// Attach an image (repeatable, at most 4)
    #[arg(short = 'i', long = "image", value_name = "PATH")]
    images: Vec<PathBuf>,
  },
  /// Cancel a scheduled post
  Unschedule {
    /// Scheduled post id, from `scheduled`
    id: String,
  },
}

pub(super) async fn run(api: &Api, command: AccountCommand) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    AccountCommand::Mute { user, undo } => {
      Data::Action(account::block(api, &ctx.user_ref(&user)?, true, undo).await?)
    }
    AccountCommand::Muted { page } => Data::Users(
      page
        .collect(async |r| account::blocked(api, true, &r).await)
        .await?,
    ),
    AccountCommand::FollowRequests { page } => Data::Users(
      page
        .collect(async |r| relations::requests(api, &r).await)
        .await?,
    ),
    AccountCommand::AcceptFollower { users } => {
      let users = user_refs(api, &users)?;
      Data::actions(relations::answer(api, &users, true).await?)
    }
    AccountCommand::DenyFollower { users } => {
      let users = user_refs(api, &users)?;
      Data::actions(relations::answer(api, &users, false).await?)
    }
    AccountCommand::Pin { post, undo } => {
      Data::Action(account::pin(api, &ctx.post_ref(&post)?, undo).await?)
    }
    AccountCommand::Scheduled => Data::Posts(account::scheduled(api).await?),
    AccountCommand::Schedule { text, at, images } => {
      let at = future_time(&at)?;
      atp_core::cli::check_images(&images)?;
      let media = uploads::images(api, &images).await?;
      Data::Action(account::schedule(api, &text, at, &media).await?)
    }
    AccountCommand::Unschedule { id } => Data::Action(account::unschedule(api, &id).await?),
  })
}

/// User arguments with `#N` resolved.
fn user_refs(api: &Api, users: &[String]) -> Result<Vec<String>> {
  users.iter().map(|u| api.ctx.user_ref(u)).collect()
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

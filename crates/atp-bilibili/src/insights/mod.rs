//! Creator-center analytics (`insights`): of the logged-in account
//! ([`account`]) and of one post ([`post`]).
//!
//! Endpoints are those of the data-center app that
//! member.bilibili.com/platform/data-up embeds (member.bilibili.com/york/data-center-web,
//! `creator-monorepo/data-center-web` bundle `index` and lazy chunks 55, 782,
//! 6099, 8359; 2026-09). Days follow Beijing time; a day's data appears the
//! next noon.

mod account;
mod post;

use atp_core::{Error, ErrorCode, Result, Value, json};

use crate::api::{Api, Call};

pub use self::account::{account, compare};
pub use self::post::post;

const DATA: &str = "https://member.bilibili.com/x/web/data";
const MEMBER: &str = "https://member.bilibili.com/";

/// A data-center GET (`/x/web/data` + `path`) as the web app sends it.
fn data<'a>(api: &'a Api, path: &str, mid: &str) -> Call<'a> {
  api
    .get(&format!("{DATA}{path}"))
    .header("referer", MEMBER)
    .arg("tmid", mid)
}

/// Send, but let a failing side request only leave a note in `errors`
/// (login, captcha and rate limits still abort).
async fn soft(call: Call<'_>, name: &str, errors: &mut Vec<Value>) -> Result<Option<Value>> {
  match call.send().await {
    Ok(v) => Ok(Some(v)),
    Err(e) if is_fatal(&e) => Err(e),
    Err(e) => {
      errors.push(json!({ "source": name, "error": e.message }));
      Ok(None)
    }
  }
}

fn is_fatal(e: &Error) -> bool {
  matches!(
    e.code,
    ErrorCode::NotAuthenticated | ErrorCode::VerificationRequired | ErrorCode::RateLimited
  )
}

/// The data center's windows: yesterday, 7, 30 or 90 days, or everything.
#[derive(Debug, Clone, Copy)]
struct Period {
  code: i8,
  days: Option<u32>,
}

impl Period {
  /// The smallest window that covers `days`.
  fn covering(days: u32) -> Self {
    let (code, days) = match days {
      0..=1 => (-1, Some(1)),
      2..=7 => (0, Some(7)),
      8..=30 => (1, Some(30)),
      31..=90 => (2, Some(90)),
      _ => (3, None),
    };
    Self { code, days }
  }

  fn label(self) -> &'static str {
    match self.code {
      -1 => "yesterday",
      0 => "7d",
      1 => "30d",
      2 => "90d",
      _ => "all",
    }
  }

  /// Trends know no "yesterday" (the web app then shows 7 days).
  fn trend(self) -> i8 {
    self.code.max(0)
  }

  /// Follower numbers know no "everything" (90 days at most).
  fn fans(self) -> i8 {
    self.code.min(2)
  }
}

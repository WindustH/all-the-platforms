//! Graph times and the windows analytics are asked over.

use atp_core::text::parse_time;
use jiff::{Timestamp, ToSpan};

/// A Graph time (`2026-10-13T09:00:00+0000`, or RFC 3339) as a timestamp.
pub fn parse(t: &str) -> Option<Timestamp> {
  parse_time(t).or_else(|| {
    jiff::fmt::strtime::parse("%Y-%m-%dT%H:%M:%S%z", t)
      .ok()?
      .to_timestamp()
      .ok()
  })
}

/// `YYYY-MM-DD` of a timestamp.
pub fn day(t: Timestamp) -> String {
  t.strftime("%Y-%m-%d").to_string()
}

/// The last `days` days (clamped to `1..=max_days`), ending now.
pub fn window(days: u32, max_days: u32) -> (Timestamp, Timestamp) {
  let days = days.clamp(1, max_days);
  let until = Timestamp::now();
  let since = until - (i64::from(days) * 24).hours();
  (since, until)
}

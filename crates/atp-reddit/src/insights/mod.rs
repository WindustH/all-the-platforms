//! Analytics. Reddit has no creator dashboard for ordinary accounts, so the
//! account's insights are what it reports natively (karma, also per
//! community, trophies, followers, age) plus daily trends computed from its
//! own posts and comments. A post's insights are its public counters, and
//! for the author also the post insights page ([`stats`]).

mod account;
mod post;
mod stats;

use std::collections::BTreeMap;

use atp_core::{Breakdown, Share};
use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;

pub use self::account::account;
pub use self::post::post;

pub(super) fn day(secs: i64, tz: &TimeZone) -> Option<Date> {
  Timestamp::from_second(secs)
    .ok()
    .map(|t| t.to_zoned(tz.clone()).date())
}

/// Slices sorted by value, largest first, with their share of the whole.
pub(super) fn breakdown(dimension: &str, counts: BTreeMap<String, i64>) -> Option<Breakdown> {
  let whole: i64 = counts.values().filter(|n| **n > 0).sum();
  let mut items: Vec<(String, i64)> = counts.into_iter().filter(|(_, n)| *n != 0).collect();
  items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
  let items: Vec<Share> = items
    .into_iter()
    .map(|(label, n)| Share {
      label,
      value: n.into(),
      ratio: (whole > 0 && n > 0).then(|| n as f64 / whole as f64),
      ..Default::default()
    })
    .collect();
  (!items.is_empty()).then(|| Breakdown {
    dimension: dimension.into(),
    items,
    ..Default::default()
  })
}

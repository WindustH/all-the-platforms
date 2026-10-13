//! Creator-center analytics (数据看板, creator.xiaohongshu.com/statistics):
//! account overview, fans, and the data of one of your notes.
//!
//! Paths come from the API list in the creator center's `index.<hash>.js`;
//! the lazy chunks of its pages call them as galaxy GETs (3074: account
//! overview, 7763: fans, 4323 / 5111: note analysis); [`metrics`] maps
//! their payloads, [`lists`] holds the data center's note and fan lists.

mod account;
mod lists;
pub mod metrics;
mod note;

use social_core::{Error, ErrorCode, Insights, Result, Value, ValueExt, json};

pub use self::account::account;
pub use self::lists::{active_fans, note_stats};
pub use self::note::note;
use crate::api::Api;
use crate::parse::snake_keys;

/// The creator center's windows: `seven` / `thirty` blocks of its payloads.
pub fn window(days: u32) -> (&'static str, u32) {
  if days <= 7 {
    ("seven", 7)
  } else {
    ("thirty", 30)
  }
}

/// A galaxy GET with snake_case keys.
pub(super) async fn get(api: &Api, path: &str, params: &[(&str, &str)]) -> Result<Value> {
  Ok(snake_keys(api.creator_get(path, params).await?))
}

/// One panel of a page: an upstream refusal only leaves a line in
/// `extra.unavailable`; captchas, rate limits and login errors still stop.
pub(super) async fn panel(
  api: &Api,
  ins: &mut Insights,
  path: &str,
  params: &[(&str, &str)],
) -> Result<Value> {
  match get(api, path, params).await {
    Ok(v) => Ok(v),
    Err(e) if soft(&e) => {
      let name = path.rsplit('/').take(2).collect::<Vec<_>>();
      unavailable(ins, &format!("{}/{}: {}", name[1], name[0], e.message));
      Ok(Value::Null)
    }
    Err(e) => Err(e),
  }
}

pub(super) fn soft(error: &Error) -> bool {
  matches!(
    error.code,
    ErrorCode::UpstreamError | ErrorCode::NotFound | ErrorCode::PermissionDenied
  )
}

pub(super) fn unavailable(ins: &mut Insights, text: &str) {
  ins.warnings.push(text.to_owned());
}

/// The rows under `key`; when there are none, the reason the page shows
/// (`<key>_tip_msg`, `no_data_tip_msg`) goes to `extra.unavailable`.
pub(super) fn rows<'a>(
  ins: &mut Insights,
  dimension: &str,
  payload: &'a Value,
  key: &str,
) -> &'a [Value] {
  let items = payload.list(key);
  let tip = payload.first_str(&[&format!("{key}_tip_msg"), "no_data_tip_msg"]);
  if let (true, Some(tip)) = (items.is_empty(), tip) {
    unavailable(ins, &format!("{dimension}: {tip}"));
  }
  items
}

/// A breakdown of the `[{title, value}]` rows under `key`.
pub(super) fn breakdown(
  ins: &mut Insights,
  dimension: &str,
  payload: &Value,
  key: &str,
  percent: bool,
) {
  let items = rows(ins, dimension, payload, key);
  metrics::breakdown(ins, dimension, items, percent);
}

/// `[{<x>, count}]` rows labelled by `x` (hour of day, second of a video).
pub(super) fn labelled(items: &[Value], x: &str, label: impl Fn(i64) -> String) -> Vec<Value> {
  items
    .iter()
    .filter_map(|i| Some(json!({"title": label(i.i64(x)?), "value": i.f64("count")?})))
    .collect()
}

/// First and last day of all series.
pub(super) fn first_last(ins: &Insights) -> Option<(String, String)> {
  let days = ins.series.iter().flat_map(|s| &s.points).map(|p| &p.date);
  Some((days.clone().min()?.clone(), days.max()?.clone()))
}

/// The first day (Beijing time) of a window of `days` days ending today.
pub(super) fn days_ago(days: u32) -> Option<String> {
  let now = crate::sign::now_ms() as i64;
  metrics::day(now - i64::from(days.saturating_sub(1)) * 86_400_000)
}

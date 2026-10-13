//! Creator analytics: totals, time series and breakdowns.

use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

use super::*;

/// One value of a trend.
#[derive(Debug, Clone, Serialize)]
pub struct Point {
  /// `YYYY-MM-DD` for daily data, RFC 3339 for finer steps.
  pub date: String,
  pub value: Value,
}

/// A metric over time, e.g. daily views.
#[derive(Debug, Clone, Serialize)]
pub struct Series {
  pub metric: String,
  pub points: Vec<Point>,
}

/// One slice of a distribution, e.g. `search` in traffic sources.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Share {
  pub label: String,
  /// Id of what the slice stands for (a post, a region code ...), when it has one.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub id: Option<String>,
  pub value: Value,
  /// Fraction of the whole (0..=1) when the platform reports or implies it.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub ratio: Option<f64>,
}

/// A distribution over one dimension: traffic source, age, gender, region, device ...
#[derive(Debug, Clone, Default, Serialize)]
pub struct Breakdown {
  pub dimension: String,
  /// What the numbers cover when it differs from the insights' `from`..`to`:
  /// `lifetime`, `30d`, `yesterday` ...
  #[serde(skip_serializing_if = "Option::is_none")]
  pub period: Option<String>,
  pub items: Vec<Share>,
}

/// Analytics of the logged-in account or of one of its posts, as the
/// platform's creator center reports them.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Insights {
  /// `account` or `post`.
  pub kind: String,
  /// The account or post the numbers are about.
  pub subject: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub title: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub url: Option<String>,
  /// First and last day covered, `YYYY-MM-DD`.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub from: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub to: Option<String>,
  /// What `totals` cover when it differs from `from`..`to` (for a post
  /// usually `lifetime`).
  #[serde(skip_serializing_if = "Option::is_none")]
  pub totals_period: Option<String>,
  /// Headline numbers: `views`, `likes`, `new_followers`, `avg_watch_seconds`,
  /// `completion_rate` ... (snake_case, rates as 0..=1 fractions).
  #[serde(skip_serializing_if = "BTreeMap::is_empty")]
  pub totals: BTreeMap<String, Value>,
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub series: Vec<Series>,
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub breakdowns: Vec<Breakdown>,
  /// Why data is missing or partial: a paid tier, a creator level, a
  /// follower threshold, a panel that failed ... (sentences for people).
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub warnings: Vec<String>,
  #[serde(skip_serializing_if = "Extra::is_empty")]
  pub extra: Extra,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub raw: Option<Value>,
}

impl Insights {
  /// Set a headline number; skipped when the platform did not report it.
  pub fn total(&mut self, metric: &str, value: impl Into<Option<Value>>) {
    if let Some(v) = value.into().filter(|v| !v.is_null()) {
      self.totals.insert(metric.to_owned(), v);
    }
  }
}

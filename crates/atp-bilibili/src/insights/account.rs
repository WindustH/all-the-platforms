//! The logged-in account's analytics and the comparison of its videos.

use atp_core::{Insights, Page, Post, Result, Value, ValueExt, json};

use super::{Period, data, soft};
use crate::api::Api;
use crate::parse::insights::{self as pi, AGES, DEVICES, GENDERS, VIEWERS};
use crate::refs::{self, Video};
use crate::users;

/// `(data-center key, metric)` of the overview numbers, by the `tab` that serves them.
const TABS: &[(u8, &[(&str, &str)])] = &[
  (
    0,
    &[
      ("play", "views"),
      ("visitor", "profile_visitors"),
      ("fan", "net_followers"),
      ("vt", "watch_minutes"),
    ],
  ),
  (
    1,
    &[("like", "likes"), ("fav", "favorites"), ("coin", "coins")],
  ),
  (
    2,
    &[
      ("comment", "comments"),
      ("dm", "danmaku"),
      ("share", "shares"),
    ],
  ),
];

/// Follower numbers (`v3/fans/stat/num`, `export`) and their metrics.
const FANS: &[(&str, &str)] = &[
  ("total", "followers"),
  ("follow", "new_followers"),
  ("unfollow", "lost_followers"),
  ("active", "active_followers"),
];

/// Analytics of the logged-in account: headline numbers against the previous
/// period, daily trends, where plays and followers come from, and the
/// follower portrait.
pub async fn account(api: &Api, days: u32) -> Result<Insights> {
  let mid = users::my_mid(api).await?;
  let period = Period::covering(days);
  let mut insights = Insights {
    kind: "account".into(),
    subject: mid.clone(),
    url: Some(refs::space_url(&mid)),
    ..Insights::default()
  };
  let mut got = Gathered::default();
  let log_date = overview(api, &mid, period, &mut insights, &mut got).await?;
  // The window: `days` of data up to the last finished day.
  let to = log_date.or_else(|| pi::days_before(&pi::today(), 1));
  let from = match (period.days, &to) {
    (Some(n), Some(to)) => pi::days_before(to, n - 1),
    _ => None,
  };
  follower_totals(api, &mid, period, &mut insights, &mut got).await?;
  trends(api, &mid, period, from.as_deref(), &mut insights, &mut got).await?;
  follower_trends(api, &mid, period, from.as_deref(), &mut insights, &mut got).await?;
  sources(api, &mid, &mut insights, &mut got).await?;
  portrait(api, &mid, &mut insights, &mut got).await?;
  drop_unavailable(&mut insights, &mut got.previous, period);
  insights.from = from.or_else(|| {
    let firsts = insights.series.iter().filter_map(|s| s.points.first());
    firsts.map(|p| p.date.clone()).min()
  });
  insights.to = to;
  annotate(&mut insights, period, got);
  Ok(insights)
}

/// What the requests collected besides the insights: upstream payloads, the
/// previous period's numbers and failed side requests.
#[derive(Default)]
struct Gathered {
  raw: serde_json::Map<String, Value>,
  previous: serde_json::Map<String, Value>,
  errors: Vec<Value>,
}

/// Headline numbers of three tabs, each with the previous period (`*_last`);
/// returns the last day with data.
async fn overview(
  api: &Api,
  mid: &str,
  period: Period,
  i: &mut Insights,
  got: &mut Gathered,
) -> Result<Option<String>> {
  let mut log_date = None;
  for (tab, keys) in TABS {
    let v = data(api, "/v2/overview/stat/num", mid)
      .arg("period", period.code)
      .arg("s_locale", "zh_CN")
      .arg("tab", tab)
      .send()
      .await?;
    for (key, metric) in *keys {
      i.total(metric, v.f64(key).map(pi::num));
      if let Some(last) = v.f64(&format!("{key}_last")) {
        got.previous.insert((*metric).into(), pi::num(last));
      }
    }
    log_date = log_date.or_else(|| pi::day(v.at("log_date")));
    got.raw.insert(format!("overview_stat_num_{tab}"), v);
  }
  Ok(log_date)
}

async fn follower_totals(
  api: &Api,
  mid: &str,
  period: Period,
  i: &mut Insights,
  got: &mut Gathered,
) -> Result<()> {
  let call = data(api, "/v3/fans/stat/num", mid).arg("period", period.fans());
  if let Some(v) = soft(call, "fans_stat_num", &mut got.errors).await? {
    for (key, metric) in FANS {
      i.total(metric, v.f64(&format!("{key}_num")).map(pi::num));
    }
    got.raw.insert("fans_stat_num".into(), v);
  }
  Ok(())
}

/// Daily trends: all overview metrics in one request (as the web export does).
async fn trends(
  api: &Api,
  mid: &str,
  period: Period,
  from: Option<&str>,
  i: &mut Insights,
  got: &mut Gathered,
) -> Result<()> {
  let keys: Vec<&str> = TABS
    .iter()
    .flat_map(|(_, k)| k.iter().map(|(k, _)| *k))
    .collect();
  let call = data(api, "/v2/overview/stat/graph", mid)
    .arg("period", period.trend())
    .arg("s_locale", "zh_CN")
    .arg("type", keys.join(","));
  if let Some(v) = soft(call, "overview_stat_graph", &mut got.errors).await? {
    for (_, metrics) in TABS {
      for (key, metric) in *metrics {
        let list = v.list(&format!("data_tendency.{key}"));
        i.series.extend(pi::series(metric, list, from));
      }
    }
    got.raw.insert("overview_stat_graph".into(), v);
  }
  Ok(())
}

async fn follower_trends(
  api: &Api,
  mid: &str,
  period: Period,
  from: Option<&str>,
  i: &mut Insights,
  got: &mut Gathered,
) -> Result<()> {
  let call = data(api, "/v3/fans/stat/export", mid).arg("period", period.fans());
  if let Some(v) = soft(call, "fans_stat_export", &mut got.errors).await? {
    for (key, metric) in FANS {
      let list = v.list(&format!("data_tendency_{key}"));
      i.series.extend(pi::series(metric, list, from));
    }
    got.raw.insert("fans_stat_export".into(), v);
  }
  Ok(())
}

/// The watch-time index is only filled for accounts Bilibili enabled it for;
/// over all time the web app shows no visitors, and `fan` is the total.
fn drop_unavailable(
  i: &mut Insights,
  previous: &mut serde_json::Map<String, Value>,
  period: Period,
) {
  let zero = |v: Option<&Value>| v.and_then(Value::as_f64) == Some(0.0);
  let mut drop = Vec::new();
  if zero(i.totals.get("watch_minutes")) && zero(previous.get("watch_minutes")) {
    drop.push("watch_minutes");
  }
  if period.days.is_none() {
    drop.extend(["profile_visitors", "net_followers"]);
  }
  for metric in drop {
    i.totals.remove(metric);
    previous.remove(metric);
    i.series.retain(|s| s.metric != metric);
  }
}

/// The period, the fixed windows of some breakdowns, the previous period,
/// failed side requests and the upstream payloads.
fn annotate(insights: &mut Insights, period: Period, got: Gathered) {
  insights
    .extra
    .insert("period".into(), json!(period.label()));
  // Distributions the data center only has for fixed windows.
  let windows = [
    ("device", "30d"),
    ("viewer_type", "30d"),
    ("follow_source", "30d"),
    ("video_last_day", "1d"),
  ];
  let windows: serde_json::Map<String, Value> = windows
    .iter()
    .filter(|(d, _)| insights.breakdowns.iter().any(|b| b.dimension == *d))
    .map(|(d, w)| ((*d).to_owned(), json!(w)))
    .collect();
  if !windows.is_empty() {
    insights
      .extra
      .insert("breakdown_windows".into(), Value::Object(windows));
  }
  if !got.previous.is_empty() {
    insights
      .extra
      .insert("previous_period".into(), Value::Object(got.previous));
  }
  if !got.errors.is_empty() {
    insights.extra.insert("errors".into(), json!(got.errors));
  }
  insights.raw = Some(Value::Object(got.raw));
}

/// The account's videos side by side with their creator-center numbers
/// (the data center's video comparison, `archive_diagnose/compare`, chunk
/// 2853): the latest `size` ones, or the given ones.
pub async fn compare(api: &Api, videos: &[Video], size: usize) -> Result<Page<Post>> {
  let mid = users::my_mid(api).await?;
  let mut call = data(api, "/archive_diagnose/compare", &mid).arg("size", size);
  if !videos.is_empty() {
    let ids: Vec<&str> = videos.iter().map(|v| v.bvid.as_str()).collect();
    call = call.arg("compare_bvids", ids.join(","));
  }
  let v = call.send().await?;
  Ok(Page::last(
    v.list("list").iter().map(pi::compared).collect(),
  ))
}

/// Where plays come from (terminal, followers or not, which videos) and where
/// new followers found the account.
async fn sources(api: &Api, mid: &str, insights: &mut Insights, got: &mut Gathered) -> Result<()> {
  let (raw, errors) = (&mut got.raw, &mut got.errors);
  let call = data(api, "/v2/overview/source", mid).arg("s_locale", "zh_CN");
  if let Some(v) = soft(call, "overview_source", errors).await? {
    // Terminal shares come in basis points, followers / others as plays.
    let device = pi::fields(v.at("play_proportion"), DEVICES)
      .into_iter()
      .map(|(label, bp)| (label, bp / 10_000.0));
    insights
      .breakdowns
      .extend(pi::breakdown("device", device, true));
    let viewers = pi::fields(v.at("audience_proportion"), VIEWERS);
    insights
      .breakdowns
      .extend(pi::breakdown("viewer_type", viewers, true));
    // Plays of each video on the last day of data.
    let videos = pi::pairs(v.list("single_arc_inc"), "title", "incr");
    insights
      .breakdowns
      .extend(pi::breakdown("video_last_day", videos, true));
    raw.insert("overview_source".into(), v);
  }
  let call = data(api, "/v2/fans/stat/source", mid);
  if let Some(v) = soft(call, "fans_stat_source", errors).await? {
    let items = v
      .as_object()
      .into_iter()
      .flatten()
      .filter_map(|(k, n)| Some((k.clone(), n.as_f64()?)));
    insights
      .breakdowns
      .extend(pi::breakdown("follow_source", items, true));
    raw.insert("fans_stat_source".into(), v);
  }
  Ok(())
}

/// Age, gender, region, interests and active hours of the followers.
async fn portrait(api: &Api, mid: &str, insights: &mut Insights, got: &mut Gathered) -> Result<()> {
  let (raw, errors) = (&mut got.raw, &mut got.errors);
  let call = data(api, "/v3/fans/stat/portrayal", mid).arg("type", "all");
  let Some(v) = soft(call, "fans_stat_portrayal", errors).await? else {
    return Ok(());
  };
  insights.breakdowns.extend(pi::breakdown(
    "gender",
    pi::fields(v.at("fans_gender"), GENDERS),
    false,
  ));
  insights.breakdowns.extend(pi::breakdown(
    "age",
    pi::fields(v.at("fans_age"), AGES),
    false,
  ));
  let region = pi::pairs(v.list("viewer_area"), "location", "count");
  insights
    .breakdowns
    .extend(pi::breakdown("region", region, true));
  let mut interest = pi::pairs(v.list("viewer_ty"), "tag_name", "count");
  interest.sort_by(|a, b| b.1.total_cmp(&a.1));
  interest.truncate(10);
  insights
    .breakdowns
    .extend(pi::breakdown("interest", interest, true));
  let mut hours: Vec<(u64, f64)> = v
    .list("fans_activity")
    .iter()
    .filter_map(|h| Some((h.u64("hour_key")?, h.f64("total_inc")?)))
    .collect();
  hours.sort_by_key(|(h, _)| *h);
  let hours = hours.into_iter().map(|(h, n)| (format!("{h:02}"), n));
  insights
    .breakdowns
    .extend(pi::breakdown("active_hour", hours, false));
  insights.extra.insert("portrait".into(), json!("followers"));
  raw.insert("fans_stat_portrayal".into(), v);
  Ok(())
}

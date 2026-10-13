//! Page insights day by day, and lifetime numbers of one Page post. Metrics
//! are asked one by one: Meta retires them every few releases and one
//! unknown name fails the whole request.

use atp_core::{Breakdown, Insights, Point, Result, Series, Share, Value, ValueExt, json};
use atp_meta::graph::{fields, params};
use atp_meta::time;

use crate::api::Api;
use crate::parse;
use crate::parse::graph::POST_FIELDS;
use crate::refs::graph_id;

const PAGE_METRICS: &[&str] = &[
  "page_media_view",
  "page_total_media_view_unique",
  "page_impressions",
  "page_impressions_unique",
  "page_post_engagements",
  "page_views_total",
  "page_total_actions",
  "page_follows",
  "page_daily_follows_unique",
  "page_daily_unfollows_unique",
  "page_video_views",
];
const POST_METRICS: &[&str] = &[
  "post_media_view",
  "post_total_media_view_unique",
  "post_impressions",
  "post_impressions_unique",
  "post_clicks",
  "post_reactions_by_type_total",
  "post_video_views",
];

/// The default Page's daily metrics over the last `days` (at most 90).
pub async fn account(api: &Api, days: u32) -> Result<Insights> {
  let page = api.page_id(None).await?;
  let graph = api.page_graph(&page).await?;
  let info = graph
    .get(&page, &fields("name,link,followers_count,fan_count"))
    .await?;
  let (since, until) = time::window(days, 90);
  let mut insights = Insights {
    kind: "account".into(),
    subject: page.clone(),
    title: info.str("name"),
    url: info.str("link"),
    from: Some(time::day(since)),
    to: Some(time::day(until)),
    ..Insights::default()
  };
  insights.total("followers", info.at("followers_count").clone());
  insights.total("page_likes", info.at("fan_count").clone());
  let since = since.as_second().to_string();
  let until = until.as_second().to_string();
  for &metric in PAGE_METRICS {
    let query = params(&[
      ("metric", metric),
      ("period", "day"),
      ("since", &since),
      ("until", &until),
    ]);
    match graph.get(&format!("{page}/insights"), &query).await {
      Ok(v) => add_series(&mut insights, metric, &v),
      Err(e) => insights.warnings.push(format!("{metric}: {}", e.message)),
    }
  }
  Ok(insights)
}

/// A daily metric as a series, summed into `totals` (follows as the last value).
fn add_series(insights: &mut Insights, metric: &str, v: &Value) {
  let Some(entry) = v.list("data").first() else {
    return;
  };
  let mut points = Vec::new();
  let mut sum = 0.0;
  for point in entry.list("values") {
    let value = point.at("value");
    if let Some(n) = value.as_f64() {
      sum += n;
    }
    if let Some(t) = point.str("end_time").and_then(|t| time::parse(&t)) {
      points.push(Point {
        date: time::day(t),
        value: value.clone(),
      });
    }
  }
  let Some(last) = points.last().map(|p| p.value.clone()) else {
    return;
  };
  let name = metric.trim_start_matches("page_").to_owned();
  let total = if metric == "page_follows" {
    last
  } else {
    json!(sum)
  };
  insights.total(&name, total);
  insights.series.push(Series {
    metric: name,
    points,
  });
}

/// Lifetime numbers of one Page post.
pub async fn post(api: &Api, arg: &str) -> Result<Insights> {
  let id = graph_id(arg)?;
  let graph = api.graph_for(&id).await?;
  let post = parse::graph::post(&graph.get(&id, &fields(POST_FIELDS)).await?);
  let mut insights = Insights {
    kind: "post".into(),
    subject: id.clone(),
    title: post.text.as_deref().map(|t| t.chars().take(60).collect()),
    url: post.url.clone(),
    totals_period: Some("lifetime".into()),
    ..Insights::default()
  };
  insights.total("likes", post.metrics.likes.map(Value::from));
  insights.total("comments", post.metrics.comments.map(Value::from));
  insights.total("shares", post.metrics.shares.map(Value::from));
  for &metric in POST_METRICS {
    let query = params(&[("metric", metric)]);
    match graph.get(&format!("{id}/insights"), &query).await {
      Ok(v) => add_lifetime(&mut insights, metric, &v),
      Err(e) => insights.warnings.push(format!("{metric}: {}", e.message)),
    }
  }
  Ok(insights)
}

/// A lifetime metric: a number into `totals`, a by-type object as a breakdown.
fn add_lifetime(insights: &mut Insights, metric: &str, v: &Value) {
  let Some(entry) = v.list("data").first() else {
    return;
  };
  let value = entry.at("values.0.value").clone();
  let name = metric.trim_start_matches("post_").to_owned();
  let Value::Object(by) = &value else {
    insights.total(&name, value);
    return;
  };
  insights.breakdowns.push(Breakdown {
    dimension: name,
    period: Some("lifetime".into()),
    items: by
      .iter()
      .map(|(k, v)| Share {
        label: k.to_lowercase(),
        value: v.clone(),
        ..Share::default()
      })
      .collect(),
  });
}

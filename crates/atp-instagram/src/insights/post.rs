//! Lifetime insights of one of the account's posts.

use atp_core::{Error, Insights, Result, Value, ValueExt, json};
use atp_meta::Graph;
use atp_meta::graph::{fields, params};

use crate::refs;

const METRICS: &[&str] = &[
  "views",
  "reach",
  "likes",
  "comments",
  "shares",
  "saved",
  "total_interactions",
  "profile_visits",
  "follows",
  "ig_reels_avg_watch_time",
  "ig_reels_video_view_total_time",
];

/// A post's insights. The Graph API has its own media ids: a link or
/// shortcode is looked up among the account's recent media.
pub async fn insights(graph: &Graph<'_>, account: &str, arg: &str) -> Result<Insights> {
  let id = graph_media_id(graph, account, arg).await?;
  let info = graph
    .get(
      &id,
      &fields(
        "id,caption,permalink,media_type,media_product_type,timestamp,like_count,comments_count",
      ),
    )
    .await?;
  let mut insights = Insights {
    kind: "post".into(),
    subject: id.clone(),
    title: info.str("caption").map(|c| c.chars().take(60).collect()),
    url: info.str("permalink"),
    totals_period: Some("lifetime".into()),
    ..Insights::default()
  };
  insights.total("likes", info.at("like_count").clone());
  insights.total("comments", info.at("comments_count").clone());
  for &metric in METRICS {
    let query = params(&[("metric", metric)]);
    match graph.get(&format!("{id}/insights"), &query).await {
      Ok(v) => insights.total(name(metric), value(metric, &v)),
      Err(e) => insights.warnings.push(format!("{metric}: {}", e.message)),
    }
  }
  Ok(insights)
}

/// The name a metric is reported under.
fn name(metric: &str) -> &str {
  match metric {
    "ig_reels_avg_watch_time" => "avg_watch_seconds",
    "ig_reels_video_view_total_time" => "watch_seconds",
    "saved" => "favorites",
    other => other,
  }
}

/// A metric's lifetime value; watch times (milliseconds) in seconds.
fn value(metric: &str, v: &Value) -> Value {
  let value = match v.at("data.0.values.0.value") {
    Value::Null => v.at("data.0.total_value.value").clone(),
    value => value.clone(),
  };
  if !metric.starts_with("ig_reels") {
    return value;
  }
  value.as_f64().map(|ms| json!(ms / 1000.0)).unwrap_or(value)
}

/// The Graph media id of a post argument (Graph ids as is).
async fn graph_media_id(graph: &Graph<'_>, account: &str, arg: &str) -> Result<String> {
  let arg = arg.trim();
  if arg.len() >= 15 && arg.bytes().all(|b| b.is_ascii_digit()) && arg.starts_with('1') {
    return Ok(arg.to_owned());
  }
  let code = refs::shortcode(arg)?;
  let items = graph
    .all(
      &format!("{account}/media"),
      &fields("id,shortcode,permalink"),
      500,
    )
    .await?;
  items
    .iter()
    .find(|m| {
      m.str("shortcode").as_deref() == Some(code.as_str())
        || m
          .str("permalink")
          .is_some_and(|p| p.contains(&format!("/{code}/")))
    })
    .and_then(|m| m.str("id"))
    .ok_or_else(|| {
      Error::not_found(format!(
        "post {code} is not among the account's latest 500 posts (insights cover your own posts only)"
      ))
    })
}

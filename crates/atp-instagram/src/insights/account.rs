//! Account insights: totals over the window, reach and new followers by
//! day, and follower demographics.

use atp_core::{Breakdown, Insights, Point, Result, Series, Share, Value, ValueExt};
use atp_meta::Graph;
use atp_meta::graph::{Params, fields, params};
use atp_meta::time;
use jiff::Timestamp;

/// Account metrics summed over the window.
const TOTALS: &[&str] = &[
  "views",
  "reach",
  "accounts_engaged",
  "total_interactions",
  "likes",
  "comments",
  "shares",
  "saves",
  "replies",
  "follows_and_unfollows",
  "profile_links_taps",
];
/// Metrics drawn day by day.
const SERIES: [&str; 2] = ["reach", "follower_count"];
const DEMOGRAPHICS: [&str; 4] = ["age", "gender", "country", "city"];

pub async fn insights(graph: &Graph<'_>, account: &str, days: u32) -> Result<Insights> {
  let info = graph
    .get(
      account,
      &fields("username,name,followers_count,follows_count,media_count"),
    )
    .await?;
  let (since, until) = time::window(days, 30);
  let mut insights = Insights {
    kind: "account".into(),
    subject: info.str("username").unwrap_or_else(|| account.to_owned()),
    title: info.str("name"),
    url: info
      .str("username")
      .map(|u| format!("https://www.instagram.com/{u}/")),
    from: Some(time::day(since)),
    to: Some(time::day(until)),
    ..Insights::default()
  };
  insights.total("followers", info.at("followers_count").clone());
  insights.total("following", info.at("follows_count").clone());
  insights.total("posts", info.at("media_count").clone());
  let window = Window { since, until };
  let path = format!("{account}/insights");
  totals(graph, &path, &window, &mut insights).await;
  series(graph, &path, &window, &mut insights).await;
  demographics(graph, &path, &mut insights).await;
  Ok(insights)
}

struct Window {
  since: Timestamp,
  until: Timestamp,
}

impl Window {
  /// A daily metric over the window, with `extra` parameters.
  fn query(&self, metric: &str, extra: &[(&str, &str)]) -> Params {
    let since = self.since.as_second().to_string();
    let until = self.until.as_second().to_string();
    let mut query = params(&[
      ("metric", metric),
      ("period", "day"),
      ("since", &since),
      ("until", &until),
    ]);
    query.extend(params(extra));
    query
  }
}

async fn totals(graph: &Graph<'_>, path: &str, window: &Window, insights: &mut Insights) {
  for &metric in TOTALS {
    let query = window.query(metric, &[("metric_type", "total_value")]);
    match graph.get(path, &query).await {
      Ok(v) => {
        insights.total(metric, v.at("data.0.total_value.value").clone());
        for b in v.list("data.0.total_value.breakdowns") {
          insights.breakdowns.push(breakdown(metric, b, None));
        }
      }
      Err(e) => insights.warnings.push(format!("{metric}: {}", e.message)),
    }
  }
}

async fn series(graph: &Graph<'_>, path: &str, window: &Window, insights: &mut Insights) {
  for metric in SERIES {
    let v = match graph.get(path, &window.query(metric, &[])).await {
      Ok(v) => v,
      Err(e) => {
        insights
          .warnings
          .push(format!("{metric} by day: {}", e.message));
        continue;
      }
    };
    let points: Vec<Point> = v
      .list("data.0.values")
      .iter()
      .filter_map(|p| {
        Some(Point {
          date: p.str("end_time")?.chars().take(10).collect(),
          value: p.at("value").clone(),
        })
      })
      .collect();
    if points.is_empty() {
      continue;
    }
    let name = if metric == "follower_count" {
      "new_followers"
    } else {
      metric
    };
    insights.series.push(Series {
      metric: name.into(),
      points,
    });
  }
}

async fn demographics(graph: &Graph<'_>, path: &str, insights: &mut Insights) {
  for dimension in DEMOGRAPHICS {
    let query = params(&[
      ("metric", "follower_demographics"),
      ("period", "lifetime"),
      ("metric_type", "total_value"),
      ("timeframe", "this_month"),
      ("breakdown", dimension),
    ]);
    match graph.get(path, &query).await {
      Ok(v) => {
        let name = format!("followers_{dimension}");
        for b in v.list("data.0.total_value.breakdowns") {
          insights
            .breakdowns
            .push(breakdown(&name, b, Some("lifetime")));
        }
      }
      Err(e) => insights
        .warnings
        .push(format!("follower {dimension}: {}", e.message)),
    }
  }
}

/// `{dimension_keys, results: [{dimension_values, value}]}` as a breakdown.
fn breakdown(metric: &str, b: &Value, period: Option<&str>) -> Breakdown {
  let keys: Vec<String> = b
    .list("dimension_keys")
    .iter()
    .filter_map(Value::as_str)
    .map(str::to_owned)
    .collect();
  Breakdown {
    dimension: if keys.is_empty() || metric.starts_with("followers_") {
      metric.to_owned()
    } else {
      format!("{metric}_by_{}", keys.join("_"))
    },
    period: period.map(str::to_owned),
    items: b
      .list("results")
      .iter()
      .map(|r| Share {
        label: label(r),
        value: r.at("value").clone(),
        ..Share::default()
      })
      .collect(),
  }
}

/// A result's dimension values joined (`25-34 / F`).
fn label(result: &Value) -> String {
  result
    .list("dimension_values")
    .iter()
    .map(|x| {
      x.as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| x.to_string())
    })
    .collect::<Vec<_>>()
    .join(" / ")
}

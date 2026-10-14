//! Insights (`threads_manage_insights`): your account over a window (views
//! by day, likes, replies, reposts, quotes, link clicks, followers and
//! their demographics) and the lifetime numbers of one of your posts.
//! Metrics are asked one at a time, so one Meta refuses only adds a
//! warning.

use atp_core::{Breakdown, Insights, Point, Result, Series, Share, Value, ValueExt};
use atp_meta::Graph;
use atp_meta::graph::{Params, fields, params};
use atp_meta::time;

use crate::api::Api;
use crate::posts;
use crate::refs;

const TOTALS: &[&str] = &["likes", "replies", "reposts", "quotes", "clicks"];
const POST_METRICS: &[&str] = &["views", "likes", "replies", "reposts", "quotes", "shares"];
const DEMOGRAPHICS: [&str; 4] = ["country", "city", "age", "gender"];
/// The longest window Meta reports at once.
const MAX_DAYS: u32 = 90;

pub async fn account(api: &Api, days: u32) -> Result<Insights> {
  let me = api.me().await?;
  let name = api.username().await?;
  let graph = api.graph().await?;
  let (since, until) = time::window(days, MAX_DAYS);
  let mut insights = Insights {
    kind: "account".into(),
    subject: name.clone(),
    url: Some(refs::profile_url(&name)),
    from: Some(time::day(since)),
    to: Some(time::day(until)),
    ..Insights::default()
  };
  let path = format!("{me}/threads_insights");
  let since = since.as_second().to_string();
  let until = until.as_second().to_string();
  let window = params(&[("since", &since), ("until", &until)]);
  views(&graph, &path, &window, &mut insights).await;
  for &metric in TOTALS {
    let mut query = params(&[("metric", metric)]);
    query.extend(window.clone());
    match graph.get(&path, &query).await {
      Ok(v) => insights.total(metric, v.at("data.0.total_value.value").clone()),
      Err(e) => insights.warnings.push(format!("{metric}: {}", e.message)),
    }
  }
  match graph
    .get(&path, &params(&[("metric", "followers_count")]))
    .await
  {
    Ok(v) => insights.total("followers", v.at("data.0.total_value.value").clone()),
    Err(e) => insights
      .warnings
      .push(format!("followers_count: {}", e.message)),
  }
  demographics(&graph, &path, &mut insights).await;
  Ok(insights)
}

/// Views by day, and their sum.
async fn views(graph: &Graph<'_>, path: &str, window: &Params, insights: &mut Insights) {
  let mut query = params(&[("metric", "views")]);
  query.extend(window.iter().cloned());
  let v = match graph.get(path, &query).await {
    Ok(v) => v,
    Err(e) => return insights.warnings.push(format!("views: {}", e.message)),
  };
  let points: Vec<Point> = v
    .list("data.0.values")
    .iter()
    .filter_map(|p| {
      let date = time::day(time::parse(&p.str("end_time")?)?);
      Some(Point {
        date,
        value: p.at("value").clone(),
      })
    })
    .collect();
  let sum: u64 = points.iter().filter_map(|p| p.value.as_u64()).sum();
  insights.total("views", Value::from(sum));
  insights.series.push(Series {
    metric: "views".into(),
    points,
  });
}

/// Followers by country, city, age and gender (accounts with at least 100
/// followers).
async fn demographics(graph: &Graph<'_>, path: &str, insights: &mut Insights) {
  for dimension in DEMOGRAPHICS {
    let query = params(&[
      ("metric", "follower_demographics"),
      ("breakdown", dimension),
    ]);
    let v = match graph.get(path, &query).await {
      Ok(v) => v,
      Err(e) => {
        insights
          .warnings
          .push(format!("followers by {dimension}: {}", e.message));
        continue;
      }
    };
    let items = v
      .list("data.0.total_value.breakdowns.0.results")
      .iter()
      .map(|r| Share {
        label: r.str("dimension_values.0").unwrap_or_default(),
        value: r.at("value").clone(),
        ..Share::default()
      })
      .collect();
    insights.breakdowns.push(Breakdown {
      dimension: dimension.into(),
      period: Some("lifetime".into()),
      items,
    });
  }
}

/// The lifetime numbers of one of your posts.
pub async fn post(api: &Api, arg: &str) -> Result<Insights> {
  let id = posts::id(api, arg).await?;
  let graph = api.graph().await?;
  let info = graph
    .get(&id, &fields("id,text,permalink,timestamp"))
    .await?;
  let mut insights = Insights {
    kind: "post".into(),
    subject: id.clone(),
    title: info.str("text").map(|t| t.chars().take(60).collect()),
    url: info.str("permalink"),
    totals_period: Some("lifetime".into()),
    ..Insights::default()
  };
  let path = format!("{id}/insights");
  for &metric in POST_METRICS {
    match graph.get(&path, &params(&[("metric", metric)])).await {
      Ok(v) => insights.total(metric, v.at("data.0.values.0.value").clone()),
      Err(e) => insights.warnings.push(format!("{metric}: {}", e.message)),
    }
  }
  Ok(insights)
}

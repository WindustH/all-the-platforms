//! Creator analytics from the 头条号 backend (`mp.toutiao.com`): daily
//! account trends, and one post's numbers.

use atp_core::{Insights, Point, Result, Series, Value, ValueExt};

use crate::api::Api;
use crate::refs::{self, Kind};

/// Daily metric → path inside `author_stat`.
const DAILY: &[(&str, &str)] = &[
  ("impressions", "consume_data.impression_count"),
  ("views", "consume_data.go_detail_count"),
  ("new_followers", "fans_change_count"),
  ("comments", "interaction_data.comment_count"),
  ("likes", "interaction_data.digg_count"),
];

pub async fn account(api: &Api, days: u32) -> Result<Insights> {
  let to = jiff::Zoned::now().date();
  let from = to.saturating_sub(jiff::Span::new().days(i64::from(days.saturating_sub(1))));
  let query = [
    ("from", from.to_string()),
    ("to", to.to_string()),
    ("type", "0".to_owned()),
  ];
  let v = api
    .mp("/mp/agw/statistic/v2/content/stat_trends", &query)
    .await?;
  let days = v.list("daily_stat_datas");
  let mut insights = Insights {
    kind: "account".into(),
    subject: "toutiao".into(),
    from: Some(from.to_string()),
    to: Some(to.to_string()),
    raw: Some(v.clone()),
    ..Insights::default()
  };
  for (metric, path) in DAILY {
    let series = series(days, metric, path);
    let sum: u64 = series.points.iter().filter_map(|p| p.value.as_u64()).sum();
    insights.total(metric, Value::from(sum));
    insights.series.push(series);
  }
  Ok(insights)
}

fn series(days: &[Value], metric: &str, path: &str) -> Series {
  let points = days
    .iter()
    .filter_map(|d| {
      let date = d.str("date")?;
      let value = d.at("author_stat").count(path)?;
      Some(Point {
        date,
        value: value.into(),
      })
    })
    .collect();
  Series {
    metric: (*metric).into(),
    points,
  }
}

/// One of your posts: lifetime numbers from the backend's item report.
pub async fn post(api: &Api, arg: &str) -> Result<Insights> {
  let post = refs::post(api, arg).await?;
  let kind = if post.kind == Kind::Video { "3" } else { "2" };
  let query = [("item_id", post.id.clone()), ("type", kind.to_owned())];
  let v = api.mp("/mp/agw/statistic/v2/item/info", &query).await?;
  let mut insights = Insights {
    kind: "post".into(),
    subject: post.id.clone(),
    url: Some(refs::post_url(&post.id)),
    totals_period: Some("lifetime".into()),
    raw: Some(v.clone()),
    ..Insights::default()
  };
  let stat = v.at("item_stat");
  for (metric, path) in [
    ("impressions", "consume_data.impression_count"),
    ("views", "consume_data.go_detail_count"),
    ("ctr", "consume_data.click_rate"),
    ("comments", "interaction_data.comment_count"),
    ("likes", "interaction_data.digg_count"),
  ] {
    insights.total(metric, stat.at(path).clone());
  }
  if insights.totals.is_empty() {
    insights
      .warnings
      .push("the backend's item report had none of the expected numbers (see --raw)".into());
  }
  Ok(insights)
}

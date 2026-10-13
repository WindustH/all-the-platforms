//! Analytics → `insights`.
//!
//! Account:
//! - API (`datacube`, verified accounts): `getbizsummary` (daily reads,
//!   shares, 在看, likes, comments, favorites and read sources, 30 days per
//!   call), `getusersummary` / `getusercumulate` (follower gains, losses and
//!   totals, 7 days per call). Days end yesterday, China time.
//! - Backend: `misc/useranalysis` (follower series) and the content report
//!   of `misc/appmsganalysis` (fields only partly documented: best effort).
//!
//! One post: lifetime counts from your published list (backend) and the
//! API's daily detail of the 30 days after publishing
//! (`getarticletotaldetail`, data since 2025-11-01).
mod backend;
mod official;

use std::collections::BTreeMap;

use jiff::ToSpan;
use jiff::civil::Date;
use jiff::tz::{TimeZone, offset};
use social_core::{Breakdown, Insights, Point, Result, Series, Share, Value, ValueExt, json};

use crate::api::Api;
use crate::published;

type Daily = BTreeMap<String, BTreeMap<String, f64>>;

fn china_today() -> Date {
  jiff::Timestamp::now()
    .to_zoned(TimeZone::fixed(offset(8)))
    .date()
}

/// `days` days ending yesterday.
fn window(days: u32) -> (Date, Date) {
  let to = china_today().yesterday().expect("valid date");
  let from = to
    .checked_sub(i64::from(days.max(1) - 1).days())
    .expect("valid date");
  (from, to)
}

fn chunks(from: Date, to: Date, span: i64) -> Vec<(Date, Date)> {
  let mut out = Vec::new();
  let mut start = from;
  while start <= to {
    let end = start
      .checked_add((span - 1).days())
      .map_or(to, |e| e.min(to));
    out.push((start, end));
    start = end.tomorrow().expect("valid date");
  }
  out
}

fn add(daily: &mut Daily, date: &str, metric: &str, value: Option<f64>) {
  if let Some(v) = value {
    *daily
      .entry(metric.into())
      .or_default()
      .entry(date.into())
      .or_default() += v;
  }
}

fn num(v: &Value, path: &str) -> Option<f64> {
  v.f64(path)
}

/// Series from daily values; totals as sums (or the last value for
/// running totals and rates).
fn finish(ins: &mut Insights, daily: Daily, last_only: &[&str]) {
  for (metric, points) in daily {
    let total = if last_only.contains(&metric.as_str()) {
      points.values().last().copied()
    } else {
      Some(points.values().sum())
    };
    if let Some(t) = total {
      ins.total(&metric, json!(round(t)));
    }
    ins.series.push(Series {
      metric,
      points: points
        .into_iter()
        .map(|(date, value)| Point {
          date,
          value: json!(round(value)),
        })
        .collect(),
    });
  }
}

fn round(v: f64) -> Value {
  if v.fract() == 0.0 {
    json!(v as i64)
  } else {
    json!((v * 10000.0).round() / 10000.0)
  }
}

fn breakdown(dimension: &str, counts: BTreeMap<String, f64>) -> Option<Breakdown> {
  let total: f64 = counts.values().sum();
  (total > 0.0).then(|| Breakdown {
    dimension: dimension.into(),
    period: None,
    items: counts
      .into_iter()
      .map(|(label, value)| Share {
        label,
        id: None,
        value: round(value),
        ratio: Some(value / total),
      })
      .collect(),
  })
}

/// Add a row's `read_user_source` counts by scene, leaving out the total.
fn add_sources(sources: &mut BTreeMap<String, f64>, row: &Value) {
  for s in row.list("read_user_source") {
    let label = s.str("scene_desc").unwrap_or_default();
    if label != "全部" {
      *sources.entry(label).or_insert(0.0) += s.f64("user_count").unwrap_or(0.0);
    }
  }
}

pub async fn account(api: &Api, days: u32) -> Result<Insights> {
  api.require_any()?;
  let (from, to) = window(days);
  let mut ins = Insights {
    kind: "account".into(),
    subject: api
      .ctx
      .http
      .cookie("slave_user")
      .or_else(|| api.official().credentials().ok().map(|cr| cr.appid))
      .unwrap_or_default(),
    from: Some(from.to_string()),
    to: Some(to.to_string()),
    ..Insights::default()
  };
  let mut daily = Daily::new();
  let mut done_content = false;
  let mut done_followers = false;
  if api.official().has() {
    match official::account(api, from, to, &mut daily, &mut ins).await {
      Ok(()) => {
        done_content = true;
        done_followers = true;
      }
      Err(e) => ins
        .warnings
        .push(format!("official API analytics: {}", e.message)),
    }
  }
  if api.backend().has() {
    if !done_followers {
      match backend::followers(api, from, to, &mut daily).await {
        Ok(()) => done_followers = true,
        Err(e) => ins
          .warnings
          .push(format!("follower analysis: {}", e.message)),
      }
    }
    if !done_content {
      match backend::content(api, &mut daily, &mut ins).await {
        Ok(()) => done_content = true,
        Err(e) => ins
          .warnings
          .push(format!("content analysis: {}", e.message)),
      }
    }
  }
  if !done_content && !done_followers && ins.warnings.is_empty() {
    ins.warnings.push("no analytics source answered".into());
  }
  finish(&mut ins, daily, &["followers"]);
  Ok(ins)
}

/// Insights of one of your articles: lifetime counts (backend) and the
/// API's daily detail (verified accounts).
pub async fn post(api: &Api, arg: &str) -> Result<Insights> {
  api.require_any()?;
  let article = crate::articles::read(&api.ctx, arg).await.ok();
  let target = crate::comments::target(api, arg).await?;
  let mut ins = Insights {
    kind: "post".into(),
    subject: format!("{}_{}", target.mid, target.idx),
    title: article.as_ref().and_then(|a| a.title.clone()),
    url: article.as_ref().and_then(|a| a.url.clone()),
    totals_period: Some("lifetime".into()),
    ..Insights::default()
  };
  if api.backend().has() {
    add_lifetime(api, &target, &mut ins).await;
  }
  if api.official().has() {
    let published = article.as_ref().and_then(|a| a.created_at);
    match published {
      Some(ts) => {
        let day = ts.to_zoned(TimeZone::fixed(offset(8))).date();
        if let Err(e) = official::post_detail(api, &target, day, &mut ins).await {
          ins
            .warnings
            .push(format!("official API detail: {}", e.message));
        }
      }
      None => ins
        .warnings
        .push("daily detail needs the article's link (for its publish date)".into()),
    }
  }
  Ok(ins)
}

/// Lifetime counts from your published list (backend).
async fn add_lifetime(api: &Api, target: &crate::comments::Target, ins: &mut Insights) {
  let Some(p) = published::find_own(api, &target.mid, target.idx).await else {
    ins
      .warnings
      .push("not among your 50 newest published articles: no lifetime counts".into());
    return;
  };
  let m = &p.metrics;
  ins.total("views", m.views.map(Value::from));
  ins.total("likes", m.likes.map(Value::from));
  ins.total("comments", m.comments.map(Value::from));
  ins.total("shares", m.shares.map(Value::from));
  for (k, v) in &m.other {
    ins.total(k, Some(Value::from(*v)));
  }
  if ins.title.is_none() {
    ins.title = p.title.clone();
  }
  if ins.url.is_none() {
    ins.url = p.url.clone();
  }
}

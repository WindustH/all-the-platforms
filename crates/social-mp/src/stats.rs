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

use std::collections::BTreeMap;

use jiff::ToSpan;
use jiff::civil::Date;
use jiff::tz::{TimeZone, offset};
use social_core::{Breakdown, Insights, Point, Result, Series, Share, Value, ValueExt, json};

use crate::client::Client;
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

pub async fn account(c: &Client, days: u32) -> Result<Insights> {
  c.require_any()?;
  let (from, to) = window(days);
  let mut ins = Insights {
    kind: "account".into(),
    subject: c
      .ctx
      .http
      .cookie("slave_user")
      .or_else(|| c.credentials().ok().map(|cr| cr.appid))
      .unwrap_or_default(),
    from: Some(from.to_string()),
    to: Some(to.to_string()),
    ..Insights::default()
  };
  let mut daily = Daily::new();
  let mut done_content = false;
  let mut done_followers = false;
  if c.has_api() {
    match api_account(c, from, to, &mut daily, &mut ins).await {
      Ok(()) => {
        done_content = true;
        done_followers = true;
      }
      Err(e) => ins
        .warnings
        .push(format!("official API analytics: {}", e.message)),
    }
  }
  if c.has_web() {
    if !done_followers {
      match web_followers(c, from, to, &mut daily).await {
        Ok(()) => done_followers = true,
        Err(e) => ins
          .warnings
          .push(format!("follower analysis: {}", e.message)),
      }
    }
    if !done_content {
      match web_content(c, &mut daily, &mut ins).await {
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

async fn api_account(
  c: &Client,
  from: Date,
  to: Date,
  daily: &mut Daily,
  ins: &mut Insights,
) -> Result<()> {
  let mut sources = BTreeMap::new();
  for (b, e) in chunks(from, to, 30) {
    let v = c
      .api_post(
        "/datacube/getbizsummary",
        &json!({ "begin_date": b.to_string(), "end_date": e.to_string() }),
      )
      .await?;
    for row in v.list("list") {
      let date = row.str("ref_date").unwrap_or_default();
      let d = if row.at("detail").is_object() {
        row.at("detail")
      } else {
        row
      };
      for (metric, path) in [
        ("views", "read_user"),
        ("shares", "share_user"),
        ("zaikan", "zaikan_user"),
        ("likes", "like_user"),
        ("comments", "comment_count"),
        ("favorites", "collection_user"),
        ("source_link_visits", "redirect_ori_page_user"),
        ("articles_sent", "send_page_count"),
      ] {
        add(daily, &date, metric, num(d, path));
      }
      for s in d.list("read_user_source") {
        let label = s.str("scene_desc").unwrap_or_default();
        if label != "全部" {
          *sources.entry(label).or_insert(0.0) += s.f64("user_count").unwrap_or(0.0);
        }
      }
    }
  }
  let mut follow_sources = BTreeMap::new();
  for (b, e) in chunks(from, to, 7) {
    let range = json!({ "begin_date": b.to_string(), "end_date": e.to_string() });
    let v = c.api_post("/datacube/getusersummary", &range).await?;
    for row in v.list("list") {
      let date = row.str("ref_date").unwrap_or_default();
      let new = num(row, "new_user");
      let lost = num(row, "cancel_user");
      add(daily, &date, "new_followers", new);
      add(daily, &date, "lost_followers", lost);
      add(
        daily,
        &date,
        "net_followers",
        Some(new.unwrap_or(0.0) - lost.unwrap_or(0.0)),
      );
      let label = follow_source(row.i64("user_source").unwrap_or(0));
      *follow_sources.entry(label.to_owned()).or_insert(0.0) += new.unwrap_or(0.0);
    }
    let v = c.api_post("/datacube/getusercumulate", &range).await?;
    for row in v.list("list") {
      let date = row.str("ref_date").unwrap_or_default();
      add(daily, &date, "followers", num(row, "cumulate_user"));
    }
  }
  ins.breakdowns.extend(breakdown("traffic_source", sources));
  ins
    .breakdowns
    .extend(breakdown("follow_source", follow_sources));
  Ok(())
}

fn follow_source(code: i64) -> &'static str {
  match code {
    1 => "search",
    17 => "profile_card",
    30 => "qr_code",
    57 => "article_account_name",
    100 => "ads",
    161 => "reprint",
    149 => "mini_program",
    200 => "channels",
    201 => "live",
    _ => "other",
  }
}

async fn web_followers(c: &Client, from: Date, to: Date, daily: &mut Daily) -> Result<()> {
  let v = c
    .web_get(
      "/misc/useranalysis",
      vec![
        ("begin_date", from.to_string()),
        ("end_date", to.to_string()),
        ("source", "99999999".into()),
      ],
    )
    .await?;
  let categories = v.list("category_list");
  let all = categories
    .iter()
    .find(|c| c.i64("user_source") == Some(99999999))
    .or_else(|| categories.first());
  for row in all.map(|c| c.list("list")).unwrap_or_default() {
    let date = row.str("date").unwrap_or_default();
    add(daily, &date, "new_followers", num(row, "new_user"));
    add(daily, &date, "lost_followers", num(row, "cancel_user"));
    add(daily, &date, "net_followers", num(row, "netgain_user"));
    add(daily, &date, "followers", num(row, "cumulate_user"));
  }
  Ok(())
}

/// The backend's content report: daily rows per metric plus summaries for
/// yesterday / 7 / 30 days.
async fn web_content(c: &Client, daily: &mut Daily, ins: &mut Insights) -> Result<()> {
  let v = c
    .web_get("/misc/appmsganalysis", vec![("action", "report".into())])
    .await?;
  let mut found = false;
  for (metric, key) in [
    ("views", "read"),
    ("shares", "share"),
    ("likes", "like"),
    ("zaikan", "zaikan"),
    ("comments", "comment"),
  ] {
    let rows = v.list(&format!("{key}_item"));
    // Rows per day and scene; 9999 (or a missing scene) is the total.
    let totals: Vec<&Value> = rows
      .iter()
      .filter(|r| matches!(r.i64("attr_scene"), None | Some(9999)))
      .collect();
    let rows: Vec<&Value> = if totals.is_empty() {
      rows.iter().collect()
    } else {
      totals
    };
    for r in rows {
      let date = r.str("ref_date").unwrap_or_default();
      add(daily, &date, metric, r.f64("uv").or_else(|| r.f64("pv")));
      found = true;
    }
    let summary = v.list(&format!("{key}_summary"));
    if !summary.is_empty() {
      let periods: Vec<Value> = summary.iter().map(|s| s.at("data_value").clone()).collect();
      ins
        .extra
        .insert(format!("{metric}_yesterday_7d_30d"), Value::from(periods));
      found = true;
    }
  }
  if !found {
    ins
      .warnings
      .push("the backend's content report had no data this CLI understands".into());
  }
  Ok(())
}

/// Insights of one of your articles: lifetime counts (backend) and the
/// API's daily detail (verified accounts).
pub async fn post(c: &Client, arg: &str) -> Result<Insights> {
  c.require_any()?;
  let article = crate::page::read(&c.ctx, arg).await.ok();
  let target = crate::comments::target(c, arg).await?;
  let mut ins = Insights {
    kind: "post".into(),
    subject: format!("{}_{}", target.mid, target.idx),
    title: article.as_ref().and_then(|a| a.title.clone()),
    url: article.as_ref().and_then(|a| a.url.clone()),
    totals_period: Some("lifetime".into()),
    ..Insights::default()
  };
  if c.has_web() {
    match published::find_own(c, &target.mid, target.idx).await {
      Some(p) => {
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
      None => ins
        .warnings
        .push("not among your 50 newest published articles: no lifetime counts".into()),
    }
  }
  if c.has_api() {
    let published = article.as_ref().and_then(|a| a.created_at);
    match published {
      Some(ts) => {
        let day = ts.to_zoned(TimeZone::fixed(offset(8))).date();
        if let Err(e) = api_post_detail(c, &target, day, &mut ins).await {
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

async fn api_post_detail(
  c: &Client,
  t: &crate::comments::Target,
  day: Date,
  ins: &mut Insights,
) -> Result<()> {
  let v = c
    .api_post(
      "/datacube/getarticletotaldetail",
      &json!({ "begin_date": day.to_string(), "end_date": day.to_string() }),
    )
    .await?;
  let msgid = format!("{}_{}", t.mid, t.idx);
  let Some(row) = v
    .list("list")
    .iter()
    .find(|r| r.str("msgid").as_deref() == Some(msgid.as_str()))
  else {
    ins.warnings.push(format!(
      "no daily detail for {msgid} (data starts 2025-11-01)"
    ));
    return Ok(());
  };
  let mut daily = Daily::new();
  let mut sources = BTreeMap::new();
  for d in row.list("detail_list") {
    let date = d.str("stat_date").unwrap_or_default();
    for (metric, path) in [
      ("views", "read_user"),
      ("shares", "share_user"),
      ("zaikan", "zaikan_user"),
      ("likes", "like_user"),
      ("comments", "comment_count"),
      ("favorites", "collection_user"),
      ("follower_views", "read_subscribe_user"),
      ("delivery_rate", "read_delivery_rate"),
      ("completion_rate", "read_finish_rate"),
      ("avg_read_minutes", "read_avg_activetime"),
    ] {
      add(&mut daily, &date, metric, d.f64(path));
    }
    for s in d.list("read_user_source") {
      let label = s.str("scene_desc").unwrap_or_default();
      if label != "全部" {
        *sources.entry(label).or_insert(0.0) += s.f64("user_count").unwrap_or(0.0);
      }
    }
  }
  // The lifetime counts of the backend win over sums of the 30 tracked days.
  let backend = ins.totals.clone();
  finish(
    ins,
    daily,
    &["delivery_rate", "completion_rate", "avg_read_minutes"],
  );
  ins.totals.extend(backend);
  ins.from = ins
    .series
    .first()
    .and_then(|s| s.points.first())
    .map(|p| p.date.clone());
  ins.to = ins
    .series
    .first()
    .and_then(|s| s.points.last())
    .map(|p| p.date.clone());
  ins.breakdowns.extend(breakdown("traffic_source", sources));
  Ok(())
}

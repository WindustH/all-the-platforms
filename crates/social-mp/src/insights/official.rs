//! Account and article analytics from the official API's `datacube`
//! (verified accounts).

use std::collections::BTreeMap;

use jiff::civil::Date;
use social_core::{Insights, Result, Value, ValueExt, json};

use super::{Daily, add, add_sources, breakdown, chunks, finish, num};
use crate::api::Api;
use crate::comments::Target;

/// Daily reads, shares, 在看 ... and read sources (`getbizsummary`), and
/// follower gains, losses and totals (`getusersummary` / `getusercumulate`).
pub async fn account(
  api: &Api,
  from: Date,
  to: Date,
  daily: &mut Daily,
  ins: &mut Insights,
) -> Result<()> {
  let sources = biz_summary(api, from, to, daily).await?;
  let follow_sources = user_summary(api, from, to, daily).await?;
  ins.breakdowns.extend(breakdown("traffic_source", sources));
  ins
    .breakdowns
    .extend(breakdown("follow_source", follow_sources));
  Ok(())
}

/// `getbizsummary` in 30-day chunks; the read sources summed.
async fn biz_summary(
  api: &Api,
  from: Date,
  to: Date,
  daily: &mut Daily,
) -> Result<BTreeMap<String, f64>> {
  let mut sources = BTreeMap::new();
  for (b, e) in chunks(from, to, 30) {
    let range = json!({ "begin_date": b.to_string(), "end_date": e.to_string() });
    let v = api
      .official()
      .post("/datacube/getbizsummary", &range)
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
      add_sources(&mut sources, d);
    }
  }
  Ok(sources)
}

/// Follower series in 7-day chunks; new followers summed by source.
async fn user_summary(
  api: &Api,
  from: Date,
  to: Date,
  daily: &mut Daily,
) -> Result<BTreeMap<String, f64>> {
  let mut follow_sources = BTreeMap::new();
  for (b, e) in chunks(from, to, 7) {
    let range = json!({ "begin_date": b.to_string(), "end_date": e.to_string() });
    let v = api
      .official()
      .post("/datacube/getusersummary", &range)
      .await?;
    for row in v.list("list") {
      let date = row.str("ref_date").unwrap_or_default();
      let new = num(row, "new_user");
      let lost = num(row, "cancel_user");
      add(daily, &date, "new_followers", new);
      add(daily, &date, "lost_followers", lost);
      let net = new.unwrap_or(0.0) - lost.unwrap_or(0.0);
      add(daily, &date, "net_followers", Some(net));
      let label = follow_source(row.i64("user_source").unwrap_or(0));
      *follow_sources.entry(label.to_owned()).or_insert(0.0) += new.unwrap_or(0.0);
    }
    let v = api
      .official()
      .post("/datacube/getusercumulate", &range)
      .await?;
    for row in v.list("list") {
      let date = row.str("ref_date").unwrap_or_default();
      add(daily, &date, "followers", num(row, "cumulate_user"));
    }
  }
  Ok(follow_sources)
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

/// The daily detail of one article (`getarticletotaldetail`) for the 30
/// days after `day`, the day it was published.
pub async fn post_detail(api: &Api, target: &Target, day: Date, ins: &mut Insights) -> Result<()> {
  let range = json!({ "begin_date": day.to_string(), "end_date": day.to_string() });
  let v = api
    .official()
    .post("/datacube/getarticletotaldetail", &range)
    .await?;
  let msgid = format!("{}_{}", target.mid, target.idx);
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
  let (daily, sources) = detail_series(row);
  // The lifetime counts of the backend win over sums of the 30 tracked days.
  let backend = ins.totals.clone();
  finish(
    ins,
    daily,
    &["delivery_rate", "completion_rate", "avg_read_minutes"],
  );
  ins.totals.extend(backend);
  let points = ins.series.first().map(|s| &s.points);
  ins.from = points.and_then(|p| p.first()).map(|p| p.date.clone());
  ins.to = points.and_then(|p| p.last()).map(|p| p.date.clone());
  ins.breakdowns.extend(breakdown("traffic_source", sources));
  Ok(())
}

/// Daily values and summed read sources of one `getarticletotaldetail` row.
fn detail_series(row: &Value) -> (Daily, BTreeMap<String, f64>) {
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
    add_sources(&mut sources, d);
  }
  (daily, sources)
}

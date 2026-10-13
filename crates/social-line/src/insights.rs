//! `insights`: friends over the last days (follows, targeted reach, blocks),
//! delivery counts by kind, the audience (gender, age, area, app, how long
//! they have been friends) and the monthly quota; for one broadcast (its
//! request id), impressions, clicks and media plays.

use social_core::{Breakdown, Insights, Point, Result, Series, Share, Value, ValueExt, json};

use crate::account;
use crate::api::Api;
use crate::users;

fn day(offset: i64) -> (String, String) {
  let d = jiff::Zoned::now()
    .with_time_zone(jiff::tz::TimeZone::get("Asia/Tokyo").unwrap_or(jiff::tz::TimeZone::UTC))
    .date()
    .checked_sub(jiff::Span::new().days(offset))
    .unwrap_or_default();
  (d.strftime("%Y%m%d").to_string(), d.to_string())
}

pub async fn account(api: &Api, days: u32) -> Result<Insights> {
  let me = users::whoami(api).await?;
  let mut ins = Insights {
    kind: "account".into(),
    subject: me.id.clone(),
    title: Some(me.name.clone()),
    ..Insights::default()
  };
  followers(api, days.clamp(1, 30), &mut ins).await?;
  deliveries(api, &mut ins).await;
  audience(api, &mut ins).await;
  if let Ok(q) = account::quota(api).await {
    ins.total("quota_limit", q.get("limit").cloned());
    ins.total("quota_used", q.get("used").cloned());
  }
  Ok(ins)
}

/// Friends, targeted reach and blocks per day; statistics are ready the
/// day after, so the days run from `days` ago to yesterday.
async fn followers(api: &Api, days: u32, ins: &mut Insights) -> Result<()> {
  let mut series: Vec<(&str, &str, Vec<Point>)> = vec![
    ("followers", "followers", Vec::new()),
    ("targeted_reaches", "targetedReaches", Vec::new()),
    ("blocks", "blocks", Vec::new()),
  ];
  for offset in (1..=i64::from(days)).rev() {
    let (key, date) = day(offset);
    let v = api
      .get(&format!("/v2/bot/insight/followers?date={key}"))
      .await?;
    if v.str("status").as_deref() != Some("ready") {
      continue;
    }
    ins.from.get_or_insert(date.clone());
    for (_, field, points) in &mut series {
      if let Some(n) = v.u64(field) {
        points.push(Point {
          date: date.clone(),
          value: json!(n),
        });
      }
    }
    ins.to = Some(date);
  }
  for (metric, _, points) in series {
    if let Some(last) = points.last() {
      ins.total(metric, Some(last.value.clone()));
    }
    if !points.is_empty() {
      ins.series.push(Series {
        metric: metric.into(),
        points,
      });
    }
  }
  Ok(())
}

/// Yesterday's deliveries by kind.
async fn deliveries(api: &Api, ins: &mut Insights) {
  const KINDS: [&str; 10] = [
    "broadcast",
    "targeting",
    "autoResponse",
    "welcomeResponse",
    "chat",
    "apiBroadcast",
    "apiPush",
    "apiMulticast",
    "apiNarrowcast",
    "apiReply",
  ];
  let (key, date) = day(1);
  let delivery = api
    .get(&format!("/v2/bot/insight/message/delivery?date={key}"))
    .await
    .unwrap_or_default();
  if delivery.str("status").as_deref() != Some("ready") {
    return;
  }
  let items: Vec<Share> = KINDS
    .iter()
    .filter_map(|k| {
      Some(Share {
        label: (*k).into(),
        value: json!(delivery.u64(k)?),
        ..Share::default()
      })
    })
    .collect();
  if !items.is_empty() {
    ins.breakdowns.push(Breakdown {
      dimension: "deliveries".into(),
      period: Some(date),
      items,
    });
  }
}

/// Gender, age, area, app and friendship length of the audience.
async fn audience(api: &Api, ins: &mut Insights) {
  let demo = api
    .get("/v2/bot/insight/demographic")
    .await
    .unwrap_or_default();
  if demo.bool("available") != Some(true) {
    ins
      .warnings
      .push("LINE shows the audience only to accounts with 20 or more friends".into());
    return;
  }
  for (dimension, key, label) in [
    ("gender", "genders", "gender"),
    ("age", "ages", "age"),
    ("region", "areas", "area"),
    ("app", "appTypes", "appType"),
    ("friends_for", "subscriptionPeriods", "subscriptionPeriod"),
  ] {
    let items: Vec<Share> = demo
      .list(key)
      .iter()
      .filter_map(|e| {
        let pct = e.f64("percentage")?;
        Some(Share {
          label: e.str(label)?,
          value: json!(pct / 100.0),
          ratio: Some(pct / 100.0),
          ..Share::default()
        })
      })
      .collect();
    if !items.is_empty() {
      ins.breakdowns.push(Breakdown {
        dimension: dimension.into(),
        period: Some("lifetime".into()),
        items,
      });
    }
  }
}

/// One broadcast or narrowcast, by the request id its send returned.
pub async fn message(api: &Api, request_id: &str) -> Result<Insights> {
  let v = api
    .get(&format!(
      "/v2/bot/insight/message/event?requestId={}",
      request_id.trim()
    ))
    .await?;
  let o = v.at("overview");
  let mut ins = Insights {
    kind: "post".into(),
    subject: request_id.trim().into(),
    totals_period: Some("lifetime".into()),
    raw: Some(v.clone()),
    ..Insights::default()
  };
  for (metric, key) in [
    ("delivered", "delivered"),
    ("impressions", "uniqueImpression"),
    ("clicks", "uniqueClick"),
    ("media_plays", "uniqueMediaPlayed"),
    ("media_plays_100", "uniqueMediaPlayed100Percent"),
  ] {
    ins.total(metric, o.get(key).filter(|x| !x.is_null()).cloned());
  }
  let clicks: Vec<Share> = v
    .list("clicks")
    .iter()
    .filter_map(|c| {
      Some(Share {
        label: c.str("url")?,
        value: json!(c.u64("click").unwrap_or_default()),
        ..Share::default()
      })
    })
    .collect();
  if !clicks.is_empty() {
    ins.breakdowns.push(Breakdown {
      dimension: "clicks".into(),
      period: Some("lifetime".into()),
      items: clicks,
    });
  }
  if o.is_null() || o == &Value::Null {
    ins
      .warnings
      .push("no statistics yet (they appear about a day after sending, for 14 days)".into());
  }
  Ok(ins)
}

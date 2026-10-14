//! `insights`: the media account's orders from the 美团联盟 API's
//! `query_order`, paid in the last days (at most 90: Meituan keeps three
//! months), page by page with `scrollId`. Totals count the orders, what
//! they paid, the estimated commission (CPS `profit` plus CPA
//! `cpaProfit`) and refunds; breakdowns by status and category; a daily
//! series of orders and commission (Beijing days).

use std::collections::BTreeMap;

use atp_core::text::from_secs;
use atp_core::{Breakdown, Insights, Point, Result, Series, Share, Value, ValueExt, json};
use jiff::tz::{TimeZone, offset};

use crate::api::Api;
use crate::parse::number;

const MAX_DAYS: u32 = 90;
const PAGE: usize = 100;
const MAX_PAGES: usize = 20;
const DAY: i64 = 86_400;

pub async fn account(api: &Api, days: u32) -> Result<Insights> {
  let days = days.clamp(1, MAX_DAYS);
  let end = jiff::Timestamp::now().as_second();
  let start = end - i64::from(days) * DAY;
  let (orders, complete) = orders(api, start, end).await?;
  let mut insights = summary(&orders);
  insights.subject = api.cps.creds(&api.ctx).map(|c| c.key).unwrap_or_default();
  insights.from = from_secs(start).map(day);
  insights.to = from_secs(end).map(day);
  insights.warnings.push(
    "商品超值券包 and 省钱包 orders are not counted: Meituan lists them only when asked for by \
     business line"
      .into(),
  );
  if !complete {
    insights.warnings.push(format!(
      "stopped after {} orders; shorten --days for the rest",
      orders.len()
    ));
  }
  Ok(insights)
}

/// The orders paid between `start` and `end`, and whether that is all.
async fn orders(api: &Api, start: i64, end: i64) -> Result<(Vec<Value>, bool)> {
  let mut orders = Vec::new();
  let mut scroll: Option<String> = None;
  for _ in 0..MAX_PAGES {
    let mut body = json!({
      "startTime": start,
      "endTime": end,
      "searchType": 2,
      "page": 1,
      "limit": PAGE,
    });
    if let Some(id) = &scroll {
      body["scrollId"] = json!(id);
    }
    let v = api.cps.call(&api.ctx, "query_order", &body).await?;
    let page = match v.list("data.dataList") {
      [] => v.list("dataList"),
      list => list,
    };
    orders.extend(page.iter().cloned());
    let next = v.first_str(&["data.scrollId", "scrollId"]);
    if page.len() < PAGE || next.is_none() || next == scroll {
      return Ok((orders, true));
    }
    scroll = next;
  }
  Ok((orders, false))
}

fn summary(orders: &[Value]) -> Insights {
  let sum = |paths: &[&str]| -> f64 {
    let total: f64 = orders
      .iter()
      .map(|o| paths.iter().filter_map(|p| o.f64(p)).sum::<f64>())
      .sum();
    (total * 100.0).round() / 100.0
  };
  let mut insights = Insights {
    kind: "account".into(),
    ..Insights::default()
  };
  insights.total("orders", Some(json!(orders.len())));
  insights.total("paid", Some(json!(sum(&["payPrice"]))));
  insights.total("commission", Some(json!(sum(&["profit", "cpaProfit"]))));
  insights.total("refunded", Some(json!(sum(&["refundPrice"]))));
  insights.total(
    "refunded_commission",
    Some(json!(sum(&["refundProfit", "cpaRefundProfit"]))),
  );
  insights.breakdowns = vec![
    breakdown(orders, "status", |o| status(o.i64("status"))),
    breakdown(orders, "category", |o| o.str("categoryName")),
  ];
  insights.series = series(orders);
  insights
}

fn status(code: Option<i64>) -> Option<String> {
  let name = match code? {
    2 => "paid",
    3 => "completed",
    4 => "cancelled",
    5 => "risk_control",
    6 => "settled",
    other => return Some(other.to_string()),
  };
  Some(name.into())
}

fn breakdown(
  orders: &[Value],
  dimension: &str,
  key: impl Fn(&Value) -> Option<String>,
) -> Breakdown {
  let mut counts: BTreeMap<String, u64> = BTreeMap::new();
  for order in orders {
    *counts
      .entry(key(order).unwrap_or_else(|| "other".into()))
      .or_default() += 1;
  }
  let total = orders.len().max(1) as f64;
  Breakdown {
    dimension: dimension.into(),
    items: counts
      .into_iter()
      .map(|(label, n)| Share {
        label,
        value: json!(n),
        ratio: Some(n as f64 / total),
        ..Share::default()
      })
      .collect(),
    ..Breakdown::default()
  }
}

/// Orders and commission per Beijing day of payment.
fn series(orders: &[Value]) -> Vec<Series> {
  let mut days: BTreeMap<String, (u64, f64)> = BTreeMap::new();
  for order in orders {
    let Some(paid) = order.i64("payTime").and_then(from_secs) else {
      continue;
    };
    let entry = days.entry(day(paid)).or_default();
    entry.0 += 1;
    entry.1 += number(order, &["profit"]).unwrap_or_default()
      + number(order, &["cpaProfit"]).unwrap_or_default();
  }
  let points = |f: &dyn Fn(&(u64, f64)) -> Value| -> Vec<Point> {
    days
      .iter()
      .map(|(date, v)| Point {
        date: date.clone(),
        value: f(v),
      })
      .collect()
  };
  vec![
    Series {
      metric: "orders".into(),
      points: points(&|v| json!(v.0)),
    },
    Series {
      metric: "commission".into(),
      points: points(&|v| json!((v.1 * 100.0).round() / 100.0)),
    },
  ]
}

/// `YYYY-MM-DD` in Beijing.
fn day(t: jiff::Timestamp) -> String {
  t.to_zoned(TimeZone::fixed(offset(8))).date().to_string()
}

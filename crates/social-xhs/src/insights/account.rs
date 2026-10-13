//! The account's numbers: overview, traffic sources, viewing hours, fans
//! and, from 50 fans on, their portrait and follow sources.

use social_core::{Insights, Result, Value, ValueExt, json};

use super::{breakdown, first_last, get, labelled, metrics, panel, rows, unavailable, window};
use crate::api::Api;
use crate::users;

const PERMISSION: &str = "/api/galaxy/creator/datacenter/permission/query";

const ACCOUNT: &str = "/api/galaxy/v2/creator/datacenter/account/base";

const ACCOUNT_SOURCES: &str = "/api/galaxy/v2/creator/datacenter/audience/source/account";

const VIEW_HOURS: &str = "/api/galaxy/v2/creator/datacenter/audience/view/periods";

const FANS: &str = "/api/galaxy/creator/data/fans/overall_new";

const FANS_PORTRAIT: &str = "/api/galaxy/creator/data/fans_portrait_new";

const FANS_SOURCES: &str = "/api/galaxy/creator/data/fans_source";

/// `permission/query` status when the data center shows numbers (0: not enabled, 1: from tomorrow).
const HAS_DATA: i64 = 2;

/// The fans page asks for the portrait and follow sources from 50 fans on.
const PORTRAIT_MIN_FANS: u64 = 50;

pub async fn account(api: &Api, days: u32) -> Result<Insights> {
  let me = users::whoami(api).await?;
  let (key, window_days) = window(days);
  let mut ins = Insights {
    kind: "account".into(),
    subject: me.id.clone(),
    title: Some(me.name.clone()),
    url: me.url.clone(),
    ..Insights::default()
  };
  ins.extra.insert("window_days".into(), json!(window_days));
  let mut raw = serde_json::Map::new();
  // The page checks this first; the panels answer either way, with zeros.
  let permission = get(api, PERMISSION, &[]).await?;
  if permission.i64("status") != Some(HAS_DATA) {
    let tip = permission.str("tip_msg").unwrap_or_default();
    ins.warnings.push(format!(
      "the data center (数据看板) is not enabled yet, numbers may be missing: {tip}"
    ));
  }
  let [base, sources, hours] = overview(api, &mut ins, key).await?;
  let fans = fans(api, &mut ins, key, &mut raw).await?;
  if ins.from.is_none() {
    ins.from = first_last(&ins).map(|(a, _)| a);
    ins.to = first_last(&ins).map(|(_, b)| b);
  }
  for (name, v) in [
    ("permission", permission),
    ("account", base),
    ("sources", sources),
    ("view_hours", hours),
    ("fans", fans),
  ] {
    raw.insert(name.into(), v);
  }
  ins.raw = Some(Value::Object(raw));
  Ok(ins)
}

/// 账号概览: totals and series of the window, comparisons, traffic sources
/// and viewing hours. Returns the three payloads.
async fn overview(api: &Api, ins: &mut Insights, key: &str) -> Result<[Value; 3]> {
  let base = panel(api, ins, ACCOUNT, &[]).await?;
  let block = base.at(key);
  metrics::totals(ins, block, metrics::ACCOUNT);
  metrics::series(ins, block, metrics::ACCOUNT, None);
  ins.from = block.i64("begin_time").and_then(metrics::day);
  ins.to = block.i64("end_time").and_then(metrics::day);
  metrics::diagnosis(ins, "vs_similar_creators", base.list("analyse_infos"));
  let sources = panel(api, ins, ACCOUNT_SOURCES, &[]).await?;
  breakdown(ins, "traffic_source", &sources, key, true);
  // 观看时段: `{start_point, end_point, count}` per hour.
  let hours = panel(api, ins, VIEW_HOURS, &[]).await?;
  let items = labelled(rows(ins, "hour_of_day", &hours, key), "start_point", |h| {
    format!("{h:02}:00")
  });
  metrics::breakdown(ins, "hour_of_day", &items, false);
  Ok([base, sources, hours])
}

/// 粉丝数据: fan totals and series, and the portrait and follow sources
/// (kept in `raw`) once there are enough fans. Returns the fans payload.
async fn fans(
  api: &Api,
  ins: &mut Insights,
  key: &str,
  raw: &mut serde_json::Map<String, Value>,
) -> Result<Value> {
  let fans = panel(api, ins, FANS, &[]).await?;
  let block = fans.at(key);
  metrics::totals(ins, block, metrics::FANS);
  metrics::series(ins, block, metrics::FANS, None);
  if block.u64("fans_count").unwrap_or(0) < PORTRAIT_MIN_FANS {
    unavailable(
      ins,
      &format!("fan portrait: shown from {PORTRAIT_MIN_FANS} fans on"),
    );
    return Ok(fans);
  }
  let portrait = panel(api, ins, FANS_PORTRAIT, &[]).await?;
  for dimension in ["gender", "age", "city", "interest"] {
    breakdown(ins, dimension, &portrait, dimension, true);
  }
  let sources = panel(api, ins, FANS_SOURCES, &[]).await?;
  let sources = match sources {
    list @ Value::Array(_) => json!({ "list": list }),
    other => other,
  };
  breakdown(ins, "follow_source", &sources, "list", false);
  raw.insert("fans_portrait".into(), portrait);
  raw.insert("fans_sources".into(), sources);
  Ok(fans)
}

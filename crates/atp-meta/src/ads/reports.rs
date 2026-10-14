//! Performance reports (`insights`) as flat rows.

use atp_core::{Ctx, Data, Error, Result, Value, ValueExt, json};

use super::{account, rows};
use crate::graph::{Graph, Params};

const INSIGHT_FIELDS: &str = "account_name,campaign_name,adset_name,ad_name,impressions,reach,frequency,clicks,unique_clicks,ctr,cpc,cpm,spend,actions,action_values,cost_per_action_type,purchase_roas";

const EX_INSIGHTS: &str = "\
Examples:
  atp fb ads insights --date-preset last_7d                  # the ad account, one row
  atp fb ads insights --level campaign --time-increment 1 -f csv > daily.csv
  atp fb ads insights 120211234567890 --breakdowns age,gender --fields impressions,clicks,spend
  atp fb ads insights --level ad --since 2026-09-01 --until 2026-09-30 -f jsonl

Fields and breakdowns are the Marketing API's names. List-valued metrics
(actions, cost_per_action_type ...) come back as {action_type: value} maps.";

#[derive(Debug, clap::Subcommand)]
pub enum ReportCommand {
  /// Performance report as rows (ad account by default, or one object)
  #[command(after_long_help = EX_INSIGHTS)]
  Insights(Report),
}

#[derive(Debug, clap::Args)]
pub struct Report {
  /// Campaign, ad set or ad id (default: the ad account)
  object: Option<String>,
  /// Ad account id (`act_` optional; default: META_AD_ACCOUNT_ID or `ads use`)
  #[arg(short, long, value_name = "ID")]
  account: Option<String>,
  /// account, campaign, adset or ad
  #[arg(long)]
  level: Option<String>,
  /// today, yesterday, last_7d, last_30d, this_month, last_month, maximum ...
  #[arg(long, default_value = "last_7d")]
  date_preset: String,
  /// First day, YYYY-MM-DD (with --until; replaces --date-preset)
  #[arg(long)]
  since: Option<String>,
  /// Last day, YYYY-MM-DD (with --since)
  #[arg(long)]
  until: Option<String>,
  /// Comma-separated fields
  #[arg(long, default_value = INSIGHT_FIELDS)]
  fields: String,
  /// Comma-separated breakdowns: age, gender, country, region,
  /// publisher_platform, platform_position, device_platform ...
  #[arg(long)]
  breakdowns: Option<String>,
  /// 1 for daily rows, 7 weekly, monthly, all_days
  #[arg(long)]
  time_increment: Option<String>,
  /// How many rows (0 = all)
  #[arg(short = 'n', long, default_value_t = 500)]
  limit: usize,
}

pub async fn run(ctx: &Ctx, graph: &Graph<'_>, command: ReportCommand) -> Result<Data> {
  let ReportCommand::Insights(report) = command;
  let target = match report.object {
    Some(object) => object,
    None => account(ctx, report.account.as_deref())?,
  };
  let mut query: Params = vec![("fields".into(), report.fields)];
  match (report.since, report.until) {
    (Some(since), Some(until)) => query.push((
      "time_range".into(),
      json!({"since": since, "until": until}).to_string(),
    )),
    (None, None) => query.push(("date_preset".into(), report.date_preset)),
    _ => return Err(Error::input("--since and --until go together")),
  }
  for (key, value) in [
    ("level", report.level),
    ("breakdowns", report.breakdowns),
    ("time_increment", report.time_increment),
  ] {
    if let Some(value) = value {
      query.push((key.into(), value));
    }
  }
  let items = graph
    .all(&format!("{target}/insights"), &query, report.limit)
    .await?;
  Ok(rows(items.into_iter().map(flatten_actions).collect()))
}

/// `[{action_type, value}]` lists as `{action_type: value}` maps, which read
/// (and flatten into CSV columns) far better.
fn flatten_actions(mut row: Value) -> Value {
  let Value::Object(columns) = &mut row else {
    return row;
  };
  for value in columns.values_mut() {
    let Value::Array(items) = value else {
      continue;
    };
    let actions = items
      .iter()
      .all(|i| i.get("action_type").is_some() && i.get("value").is_some());
    if !actions {
      continue;
    }
    let map: serde_json::Map<String, Value> = items
      .iter()
      .filter_map(|i| Some((i.str("action_type")?, i.get("value")?.clone())))
      .collect();
    *value = Value::Object(map);
  }
  row
}

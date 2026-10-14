//! `atp tiktok ads ...`: ad accounts, campaigns, ad groups, ads, audiences,
//! pixels and performance reports. Lists come as rows (one per object) for
//! JSON Lines / CSV; report rows merge their dimensions and metrics.

use atp_core::credentials::env;
use atp_core::{Data, Error, Result, Value, ValueExt, json};

use super::Ads;
use crate::api::Api;

const EX_REPORT: &str = "\
Examples:
  atp tiktok ads report --days 30                       # spend, clicks ... per campaign and day
  atp tiktok ads report --level ad --metrics spend,impressions,clicks,ctr --from 2026-09-01 --to 2026-09-30 -f csv
  atp tiktok ads report --type AUDIENCE --dimensions gender,age --metrics spend,impressions

Dimensions and metrics are the Marketing API's names (report/integrated/get).";

const EX_RAW: &str = "\
Examples:
  atp tiktok ads raw /campaign/get/ advertiser_id=7012345678901234567 page_size=10
  atp tiktok ads raw /adgroup/get/ advertiser_id=7012345678901234567 'filtering={\"campaign_ids\":[\"1800000000000001\"]}'

Values that parse as JSON are sent as JSON; --post sends them as a JSON body.";

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// The person the access token belongs to
  Me,
  /// Ad accounts the token manages (needs TIKTOK_ADS_APP_ID and TIKTOK_ADS_SECRET)
  Advertisers,
  /// Details of ad accounts (default: the current one)
  Advertiser {
    /// Ad account (advertiser) ids
    ids: Vec<String>,
  },
  /// Remember the ad account to use by default
  Use {
    /// Ad account (advertiser) id
    advertiser: String,
  },
  /// Campaigns of an ad account
  Campaigns {
    #[command(flatten)]
    scope: Scope,
  },
  /// Ad groups, optionally of some campaigns
  Adgroups {
    #[command(flatten)]
    scope: Scope,
    /// Only these campaigns (repeatable)
    #[arg(long = "campaign", value_name = "ID")]
    campaigns: Vec<String>,
  },
  /// Ads, optionally of some campaigns or ad groups
  Ads {
    #[command(flatten)]
    scope: Scope,
    /// Only these campaigns (repeatable)
    #[arg(long = "campaign", value_name = "ID")]
    campaigns: Vec<String>,
    /// Only these ad groups (repeatable)
    #[arg(long = "adgroup", value_name = "ID")]
    adgroups: Vec<String>,
  },
  /// Custom and lookalike audiences
  Audiences {
    #[command(flatten)]
    scope: Scope,
  },
  /// Pixels of an ad account
  Pixels {
    #[command(flatten)]
    scope: Scope,
  },
  /// Performance report (synchronous `report/integrated/get`)
  #[command(after_long_help = EX_REPORT)]
  Report(ReportArgs),
  /// Any Marketing API endpoint: `ads raw /campaign/get/ advertiser_id=1 page_size=10`
  #[command(after_long_help = EX_RAW)]
  Raw {
    /// Path after /open_api/v1.3, e.g. /campaign/get/
    path: String,
    /// key=value parameters (values that parse as JSON are sent as JSON)
    params: Vec<String>,
    /// Send as POST with the parameters as a JSON body
    #[arg(long)]
    post: bool,
  },
}

#[derive(Debug, clap::Args)]
pub struct ReportArgs {
  #[command(flatten)]
  scope: Scope,
  /// advertiser, campaign, adgroup or ad
  #[arg(long, default_value = "campaign")]
  level: String,
  /// BASIC, AUDIENCE, PLAYABLE_MATERIAL or CATALOG
  #[arg(long = "type", default_value = "BASIC")]
  report_type: String,
  /// Comma-separated dimensions (default: <level>_id,stat_time_day)
  #[arg(long, value_delimiter = ',')]
  dimensions: Vec<String>,
  /// Comma-separated metrics
  #[arg(
    long,
    value_delimiter = ',',
    default_value = "spend,impressions,clicks,ctr,cpc,cpm,reach,conversion,cost_per_conversion"
  )]
  metrics: Vec<String>,
  /// First day, YYYY-MM-DD (default: --days ago)
  #[arg(long)]
  from: Option<String>,
  /// Last day, YYYY-MM-DD (default: yesterday)
  #[arg(long)]
  to: Option<String>,
  /// Days back when --from is not given
  #[arg(long, default_value_t = 7)]
  days: u32,
  /// Filtering as the API's JSON (e.g. '[{"field_name":"campaign_ids","filter_type":"IN","filter_value":"[\"1\"]"}]')
  #[arg(long, value_name = "JSON")]
  filter: Option<String>,
}

#[derive(Debug, clap::Args)]
pub struct Scope {
  /// Ad account (advertiser) id
  #[arg(short, long, value_name = "ID")]
  advertiser: Option<String>,
  /// How many rows (0 = all)
  #[arg(short = 'n', long, default_value_t = 100)]
  limit: usize,
  /// Only objects with this primary status (e.g. STATUS_DELIVERY_OK)
  #[arg(long)]
  status: Option<String>,
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  let ads = Ads::new(api)?;
  Ok(match command {
    Command::Me => Data::User(Box::new(ads.me().await?)),
    Command::Advertisers => rows(advertisers(&ads).await?),
    Command::Advertiser { ids } => rows(advertiser(&ads, ids).await?),
    Command::Use { advertiser } => {
      ads.remember_advertiser(advertiser.trim());
      Data::Action(
        atp_core::Action::done("use", advertiser.trim())
          .with_message("saved as the default ad account"),
      )
    }
    Command::Campaigns { scope } => rows(objects(&ads, "/campaign/get/", &scope, json!({})).await?),
    Command::Adgroups { scope, campaigns } => {
      let filtering = with_ids(json!({}), "campaign_ids", &campaigns);
      rows(objects(&ads, "/adgroup/get/", &scope, filtering).await?)
    }
    Command::Ads {
      scope,
      campaigns,
      adgroups,
    } => {
      let filtering = with_ids(
        with_ids(json!({}), "campaign_ids", &campaigns),
        "adgroup_ids",
        &adgroups,
      );
      rows(objects(&ads, "/ad/get/", &scope, filtering).await?)
    }
    Command::Audiences { scope } => rows(audiences(&ads, &scope).await?),
    Command::Pixels { scope } => rows(pixels(&ads, &scope).await?),
    Command::Report(args) => rows(report(&ads, args).await?),
    Command::Raw { path, params, post } => Data::Value(raw(&ads, &path, &params, post).await?),
  })
}

fn rows(items: Vec<Value>) -> Data {
  Data::Value(Value::Array(items))
}

/// Ad accounts the token manages; this listing needs the app's id and secret.
async fn advertisers(ads: &Ads<'_>) -> Result<Vec<Value>> {
  let (Some(app), Some(secret)) = (env("TIKTOK_ADS_APP_ID"), env("TIKTOK_ADS_SECRET")) else {
    return Err(Error::input(
      "listing ad accounts needs TIKTOK_ADS_APP_ID and TIKTOK_ADS_SECRET",
    ));
  };
  let v = ads
    .get(
      "/oauth2/advertiser/get/",
      &[
        ("app_id", app.into()),
        ("secret", secret.into()),
        ("access_token", ads.token.clone().into()),
      ],
    )
    .await?;
  Ok(v.list("list").to_vec())
}

/// Details of the given ad accounts, or of the current one.
async fn advertiser(ads: &Ads<'_>, ids: Vec<String>) -> Result<Vec<Value>> {
  let ids = if ids.is_empty() {
    vec![ads.advertiser(None)?]
  } else {
    ids
  };
  let v = ads
    .get("/advertiser/info/", &[("advertiser_ids", json!(ids))])
    .await?;
  Ok(v.list("list").to_vec())
}

async fn audiences(ads: &Ads<'_>, scope: &Scope) -> Result<Vec<Value>> {
  let id = ads.advertiser(scope.advertiser.as_deref())?;
  ads
    .list(
      "/dmp/custom_audience/list/",
      &[("advertiser_id", id.into())],
      scope.limit,
    )
    .await
}

async fn pixels(ads: &Ads<'_>, scope: &Scope) -> Result<Vec<Value>> {
  let id = ads.advertiser(scope.advertiser.as_deref())?;
  let v = ads
    .get(
      "/pixel/list/",
      &[("advertiser_id", id.into()), ("page_size", json!(20))],
    )
    .await?;
  Ok(v.list("pixels").to_vec())
}

/// Report rows: one per dimension combination, dimensions then metrics.
async fn report(ads: &Ads<'_>, args: ReportArgs) -> Result<Vec<Value>> {
  let id = ads.advertiser(args.scope.advertiser.as_deref())?;
  let (data_level, key) = match args.level.as_str() {
    "advertiser" => ("AUCTION_ADVERTISER", "advertiser_id"),
    "campaign" => ("AUCTION_CAMPAIGN", "campaign_id"),
    "adgroup" => ("AUCTION_ADGROUP", "adgroup_id"),
    "ad" => ("AUCTION_AD", "ad_id"),
    other => {
      return Err(Error::input(format!(
        "unknown level `{other}`: advertiser, campaign, adgroup or ad"
      )));
    }
  };
  let dimensions = if args.dimensions.is_empty() {
    vec![key.to_owned(), "stat_time_day".to_owned()]
  } else {
    args.dimensions
  };
  let (from, to) = dates(args.from, args.to, args.days)?;
  let mut params = vec![
    ("advertiser_id", Value::from(id)),
    ("report_type", args.report_type.into()),
    ("data_level", data_level.into()),
    ("dimensions", json!(dimensions)),
    ("metrics", json!(args.metrics)),
    ("start_date", from.into()),
    ("end_date", to.into()),
  ];
  if let Some(filter) = args.filter {
    let parsed: Value = serde_json::from_str(&filter)
      .map_err(|e| Error::input(format!("--filter is not JSON: {e}")))?;
    params.push(("filtering", parsed));
  }
  let list = ads
    .list("/report/integrated/get/", &params, args.scope.limit)
    .await?;
  Ok(list.iter().map(report_row).collect())
}

/// Any endpoint, with `key=value` parameters (JSON values sent as JSON).
async fn raw(ads: &Ads<'_>, path: &str, params: &[String], post: bool) -> Result<Value> {
  let mut body = serde_json::Map::new();
  for param in params {
    let (k, v) = param
      .split_once('=')
      .ok_or_else(|| Error::input(format!("`{param}` is not key=value")))?;
    let value = serde_json::from_str(v).unwrap_or_else(|_| Value::from(v));
    body.insert(k.to_owned(), value);
  }
  let path = format!("/{}", path.trim_start_matches('/'));
  if post {
    ads.post(&path, &Value::Object(body)).await
  } else {
    let pairs: Vec<(&str, Value)> = body.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    ads.get(&path, &pairs).await
  }
}

/// `campaign/get`-style listings with a `filtering` object.
async fn objects(
  ads: &Ads<'_>,
  path: &str,
  scope: &Scope,
  mut filtering: Value,
) -> Result<Vec<Value>> {
  let id = ads.advertiser(scope.advertiser.as_deref())?;
  if let Some(status) = &scope.status {
    filtering["primary_status"] = status.clone().into();
  }
  let mut params = vec![("advertiser_id", Value::from(id))];
  if filtering.as_object().is_some_and(|m| !m.is_empty()) {
    params.push(("filtering", filtering));
  }
  ads.list(path, &params, scope.limit).await
}

fn with_ids(mut filtering: Value, key: &str, ids: &[String]) -> Value {
  if !ids.is_empty() {
    filtering[key] = json!(ids);
  }
  filtering
}

/// One flat row: the dimensions, then the metrics.
fn report_row(v: &Value) -> Value {
  let mut row = serde_json::Map::new();
  for part in ["dimensions", "metrics"] {
    if let Some(m) = v.at(part).as_object() {
      for (k, val) in m {
        row.insert(k.clone(), val.clone());
      }
    }
  }
  Value::Object(row)
}

/// The report window: explicit days, or the `days` before today.
fn dates(from: Option<String>, to: Option<String>, days: u32) -> Result<(String, String)> {
  let today = jiff::Zoned::now().date();
  let day = |offset: i64| {
    today
      .checked_sub(jiff::Span::new().days(offset))
      .map(|d| d.to_string())
      .map_err(|e| Error::input(e.to_string()))
  };
  let to = match to {
    Some(t) => t,
    None => day(1)?,
  };
  let from = match from {
    Some(f) => f,
    None => day(i64::from(days.max(1)))?,
  };
  for d in [&from, &to] {
    if d.parse::<jiff::civil::Date>().is_err() {
      return Err(Error::input(format!("`{d}` is not a YYYY-MM-DD date")));
    }
  }
  Ok((from, to))
}

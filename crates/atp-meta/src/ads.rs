//! The Marketing API (`atp facebook ads ...`, `atp instagram ads ...`):
//! ad accounts, campaigns, ad sets, ads, creatives, audiences, pixels, and
//! insights reports as flat rows, plus `raw` for any other endpoint. It
//! covers what Meta's own Ads CLI does, over the same Graph API.
//!
//! The ad account: `-a`, `META_AD_ACCOUNT_ID`, or the one saved by `ads use`.
//! New campaigns start paused; `activate` turns them on.

use atp_core::{Action, Ctx, Data, Error, Result, User, Value, ValueExt, json};

use crate::graph::{Graph, Params};

const ACCOUNT_KEY: &str = "ad_account";

const ACCOUNT_FIELDS: &str = "id,account_id,name,account_status,currency,timezone_name,amount_spent,balance,spend_cap,business{id,name}";
const CAMPAIGN_FIELDS: &str = "id,name,status,effective_status,objective,buying_type,daily_budget,lifetime_budget,budget_remaining,start_time,stop_time,created_time,updated_time";
const ADSET_FIELDS: &str = "id,name,status,effective_status,campaign_id,daily_budget,lifetime_budget,optimization_goal,billing_event,bid_amount,start_time,end_time,created_time,updated_time";
const AD_FIELDS: &str = "id,name,status,effective_status,campaign_id,adset_id,creative{id,name},created_time,updated_time";
const CREATIVE_FIELDS: &str = "id,name,title,body,object_type,status,thumbnail_url,image_url,call_to_action_type,effective_object_story_id";
const AUDIENCE_FIELDS: &str = "id,name,subtype,description,approximate_count_lower_bound,approximate_count_upper_bound,delivery_status,operation_status,time_created,time_updated";
const PIXEL_FIELDS: &str = "id,name,creation_time,last_fired_time,is_unavailable";
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
pub enum Command {
  /// The person or system user the access token belongs to
  Me,
  /// Ad accounts the token can use
  Accounts {
    #[arg(short = 'n', long, default_value_t = 100)]
    limit: usize,
  },
  /// Details of ad accounts (default: the current one)
  Account { ids: Vec<String> },
  /// Remember the ad account to use by default
  Use { account: String },
  /// Campaigns of an ad account
  Campaigns {
    #[command(flatten)]
    scope: Scope,
  },
  /// Ad sets, optionally of one campaign
  Adsets {
    #[command(flatten)]
    scope: Scope,
    #[arg(long, value_name = "ID")]
    campaign: Option<String>,
  },
  /// Ads, optionally of one campaign or ad set
  Ads {
    #[command(flatten)]
    scope: Scope,
    #[arg(long, value_name = "ID")]
    campaign: Option<String>,
    #[arg(long, value_name = "ID")]
    adset: Option<String>,
  },
  /// Ad creatives
  Creatives {
    #[command(flatten)]
    scope: Scope,
  },
  /// Custom and lookalike audiences
  Audiences {
    #[command(flatten)]
    scope: Scope,
  },
  /// Pixels (datasets) of an ad account
  Pixels {
    #[command(flatten)]
    scope: Scope,
  },
  /// Any object by id (campaign, ad set, ad, creative ...)
  Get {
    id: String,
    /// Comma-separated fields
    #[arg(long)]
    fields: Option<String>,
  },
  /// Create a campaign (paused unless --status ACTIVE)
  CreateCampaign {
    #[arg(short, long, value_name = "ID")]
    account: Option<String>,
    #[arg(long)]
    name: String,
    /// OUTCOME_AWARENESS, OUTCOME_TRAFFIC, OUTCOME_ENGAGEMENT, OUTCOME_LEADS,
    /// OUTCOME_APP_PROMOTION or OUTCOME_SALES
    #[arg(long)]
    objective: String,
    #[arg(long, default_value = "PAUSED")]
    status: String,
    /// Daily budget in the account currency's minor unit (cents)
    #[arg(long)]
    daily_budget: Option<u64>,
    #[arg(long)]
    lifetime_budget: Option<u64>,
    /// HOUSING, EMPLOYMENT, CREDIT, ISSUES_ELECTIONS_POLITICS ... (repeatable; none by default)
    #[arg(long = "special-ad-category")]
    special: Vec<String>,
  },
  /// Change fields of an object: `ads update ID name=Spring daily_budget=5000`
  Update {
    id: String,
    /// key=value pairs
    #[arg(required = true)]
    fields: Vec<String>,
  },
  /// Pause campaigns, ad sets or ads
  Pause { ids: Vec<String> },
  /// Turn campaigns, ad sets or ads on
  Activate { ids: Vec<String> },
  /// Delete campaigns, ad sets, ads, creatives or audiences
  Delete {
    ids: Vec<String>,
    #[arg(short, long)]
    yes: bool,
  },
  /// Performance report as rows (ad account by default, or one object)
  #[command(after_long_help = EX_INSIGHTS)]
  Insights {
    /// Campaign, ad set or ad id (default: the ad account)
    object: Option<String>,
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
    #[arg(short = 'n', long, default_value_t = 500)]
    limit: usize,
  },
  /// Any Graph endpoint: `ads raw act_123/campaigns fields=name,status`
  Raw {
    /// Path after the version, e.g. act_123/campaigns or me/adaccounts
    path: String,
    /// key=value parameters
    params: Vec<String>,
    /// GET, POST or DELETE
    #[arg(long, default_value = "GET")]
    method: String,
  },
}

#[derive(Debug, clap::Args)]
pub struct Scope {
  /// Ad account id (`act_` optional)
  #[arg(short, long, value_name = "ID")]
  account: Option<String>,
  /// How many rows (0 = all)
  #[arg(short = 'n', long, default_value_t = 100)]
  limit: usize,
  /// Only these effective statuses: ACTIVE, PAUSED, ARCHIVED ... (repeatable)
  #[arg(long)]
  status: Vec<String>,
  /// Comma-separated fields instead of the defaults
  #[arg(long)]
  fields: Option<String>,
}

/// `act_<id>` from `-a`, `META_AD_ACCOUNT_ID` or the saved choice.
fn account(ctx: &Ctx, given: Option<&str>) -> Result<String> {
  let id = given
    .map(str::to_owned)
    .or_else(|| {
      std::env::var("META_AD_ACCOUNT_ID")
        .ok()
        .filter(|v| !v.trim().is_empty())
    })
    .or_else(|| ctx.extra(ACCOUNT_KEY))
    .ok_or_else(|| {
      Error::input("which ad account? pass -a ID, set META_AD_ACCOUNT_ID or run `ads use ID`")
        .with_hint("`ads accounts` lists the ones your token can use")
    })?;
  let id = id.trim();
  Ok(if id.starts_with("act_") {
    id.to_owned()
  } else {
    format!("act_{id}")
  })
}

fn kv(pairs: &[String]) -> Result<Params> {
  pairs
    .iter()
    .map(|p| {
      p.split_once('=')
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .ok_or_else(|| Error::input(format!("expected key=value, got `{p}`")))
    })
    .collect()
}

fn rows(items: Vec<Value>) -> Data {
  Data::Value(Value::Array(items))
}

pub async fn run(ctx: &Ctx, graph: &Graph<'_>, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Me => {
      let v = graph
        .get("me", &[("fields".into(), "id,name".into())])
        .await?;
      Data::User(Box::new(User {
        id: v.str("id").unwrap_or_default(),
        name: v.str("name").unwrap_or_default(),
        extra: [("source".to_owned(), Value::from("graph-api"))].into(),
        raw: Some(v),
        ..User::default()
      }))
    }
    Command::Accounts { limit } => rows(
      graph
        .all(
          "me/adaccounts",
          &[("fields".into(), ACCOUNT_FIELDS.into())],
          limit,
        )
        .await?,
    ),
    Command::Account { ids } => {
      let ids = if ids.is_empty() {
        vec![account(ctx, None)?]
      } else {
        ids
          .iter()
          .map(|i| account(ctx, Some(i)))
          .collect::<Result<_>>()?
      };
      let mut out = Vec::new();
      for id in ids {
        out.push(
          graph
            .get(&id, &[("fields".into(), ACCOUNT_FIELDS.into())])
            .await?,
        );
      }
      rows(out)
    }
    Command::Use { account: id } => {
      let id = account(ctx, Some(&id))?;
      let v = graph
        .get(&id, &[("fields".into(), "id,name".into())])
        .await?;
      ctx.set_extra(ACCOUNT_KEY, &id);
      Data::Action(
        Action::done("use", v.str("name").unwrap_or_else(|| id.clone()))
          .with_id(id)
          .with_message("used by default from now on"),
      )
    }
    Command::Campaigns { scope } => {
      list(ctx, graph, &scope, "campaigns", CAMPAIGN_FIELDS, None).await?
    }
    Command::Adsets { scope, campaign } => {
      let parent = campaign.map(|c| format!("{c}/adsets"));
      list(ctx, graph, &scope, "adsets", ADSET_FIELDS, parent).await?
    }
    Command::Ads {
      scope,
      campaign,
      adset,
    } => {
      let parent = adset.or(campaign).map(|p| format!("{p}/ads"));
      list(ctx, graph, &scope, "ads", AD_FIELDS, parent).await?
    }
    Command::Creatives { scope } => {
      list(ctx, graph, &scope, "adcreatives", CREATIVE_FIELDS, None).await?
    }
    Command::Audiences { scope } => {
      list(ctx, graph, &scope, "customaudiences", AUDIENCE_FIELDS, None).await?
    }
    Command::Pixels { scope } => list(ctx, graph, &scope, "adspixels", PIXEL_FIELDS, None).await?,
    Command::Get { id, fields } => {
      let mut q = Params::new();
      if let Some(f) = fields {
        q.push(("fields".into(), f));
      }
      Data::Value(graph.get(&id, &q).await?)
    }
    Command::CreateCampaign {
      account: acct,
      name,
      objective,
      status,
      daily_budget,
      lifetime_budget,
      special,
    } => {
      let acct = account(ctx, acct.as_deref())?;
      let mut form: Params = vec![
        ("name".into(), name.clone()),
        ("objective".into(), objective.to_uppercase()),
        ("status".into(), status.to_uppercase()),
        (
          "special_ad_categories".into(),
          json!(special.iter().map(|s| s.to_uppercase()).collect::<Vec<_>>()).to_string(),
        ),
      ];
      if let Some(b) = daily_budget {
        form.push(("daily_budget".into(), b.to_string()));
      }
      if let Some(b) = lifetime_budget {
        form.push(("lifetime_budget".into(), b.to_string()));
      }
      let v = graph.post(&format!("{acct}/campaigns"), &form).await?;
      Data::Action(
        Action::done("create-campaign", name)
          .with_id(v.str("id").unwrap_or_default())
          .with_message(format!("status {status}")),
      )
    }
    Command::Update { id, fields } => {
      graph.post(&id, &kv(&fields)?).await?;
      Data::Action(Action::done("update", &id).with_id(id))
    }
    Command::Pause { ids } => status(graph, &ids, "PAUSED").await?,
    Command::Activate { ids } => status(graph, &ids, "ACTIVE").await?,
    Command::Delete { ids, yes } => {
      if !yes {
        return Err(Error::input(format!(
          "refusing to delete {} without --yes",
          ids.join(", ")
        )));
      }
      for id in &ids {
        graph.delete(id).await?;
      }
      Data::Action(Action::done("delete", ids.join(", ")))
    }
    Command::Insights {
      object,
      account: acct,
      level,
      date_preset,
      since,
      until,
      fields,
      breakdowns,
      time_increment,
      limit,
    } => {
      let target = match object {
        Some(o) => o,
        None => account(ctx, acct.as_deref())?,
      };
      let mut q: Params = vec![("fields".into(), fields)];
      match (since, until) {
        (Some(s), Some(u)) => {
          q.push((
            "time_range".into(),
            json!({"since": s, "until": u}).to_string(),
          ));
        }
        (None, None) => q.push(("date_preset".into(), date_preset)),
        _ => return Err(Error::input("--since and --until go together")),
      }
      for (k, v) in [
        ("level", level),
        ("breakdowns", breakdowns),
        ("time_increment", time_increment),
      ] {
        if let Some(v) = v {
          q.push((k.into(), v));
        }
      }
      let items = graph.all(&format!("{target}/insights"), &q, limit).await?;
      rows(items.into_iter().map(flatten_actions).collect())
    }
    Command::Raw {
      path,
      params,
      method,
    } => {
      let p = kv(&params)?;
      Data::Value(match method.to_uppercase().as_str() {
        "GET" => graph.get(&path, &p).await?,
        "POST" => graph.post(&path, &p).await?,
        "DELETE" => graph.delete(&path).await?,
        other => return Err(Error::input(format!("unknown method {other}"))),
      })
    }
  })
}

/// One edge of the ad account (or of `parent`), as rows.
async fn list(
  ctx: &Ctx,
  graph: &Graph<'_>,
  scope: &Scope,
  edge: &str,
  fields: &str,
  parent: Option<String>,
) -> Result<Data> {
  let path = match parent {
    Some(p) => p,
    None => format!("{}/{edge}", account(ctx, scope.account.as_deref())?),
  };
  let mut q: Params = vec![(
    "fields".into(),
    scope.fields.clone().unwrap_or_else(|| fields.to_owned()),
  )];
  if !scope.status.is_empty() {
    let statuses: Vec<String> = scope.status.iter().map(|s| s.to_uppercase()).collect();
    q.push(("effective_status".into(), json!(statuses).to_string()));
  }
  Ok(rows(graph.all(&path, &q, scope.limit).await?))
}

async fn status(graph: &Graph<'_>, ids: &[String], status: &str) -> Result<Data> {
  if ids.is_empty() {
    return Err(Error::input("give at least one id"));
  }
  for id in ids {
    graph.post(id, &[("status".into(), status.into())]).await?;
  }
  Ok(Data::Action(
    Action::done(status.to_lowercase(), ids.join(", ")).with_message(format!("status {status}")),
  ))
}

/// `[{action_type, value}]` lists as `{action_type: value}` maps, which read
/// (and flatten into CSV columns) far better.
fn flatten_actions(mut row: Value) -> Value {
  if let Value::Object(m) = &mut row {
    for (_, v) in m.iter_mut() {
      let Value::Array(items) = v else {
        continue;
      };
      if !items
        .iter()
        .all(|i| i.get("action_type").is_some() && i.get("value").is_some())
      {
        continue;
      }
      let map: serde_json::Map<String, Value> = items
        .iter()
        .filter_map(|i| Some((i.str("action_type")?, i.get("value")?.clone())))
        .collect();
      *v = Value::Object(map);
    }
  }
  row
}

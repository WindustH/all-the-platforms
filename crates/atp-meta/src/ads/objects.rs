//! The objects of an ad account: campaigns, ad sets, ads, creatives,
//! audiences, pixels, and any object by id.

use atp_core::{Ctx, Data, Result, json};

use super::{account, rows};
use crate::graph::{Graph, Params, fields};

const CAMPAIGN_FIELDS: &str = "id,name,status,effective_status,objective,buying_type,daily_budget,lifetime_budget,budget_remaining,start_time,stop_time,created_time,updated_time";
const ADSET_FIELDS: &str = "id,name,status,effective_status,campaign_id,daily_budget,lifetime_budget,optimization_goal,billing_event,bid_amount,start_time,end_time,created_time,updated_time";
const AD_FIELDS: &str = "id,name,status,effective_status,campaign_id,adset_id,creative{id,name},created_time,updated_time";
const CREATIVE_FIELDS: &str = "id,name,title,body,object_type,status,thumbnail_url,image_url,call_to_action_type,effective_object_story_id";
const AUDIENCE_FIELDS: &str = "id,name,subtype,description,approximate_count_lower_bound,approximate_count_upper_bound,delivery_status,operation_status,time_created,time_updated";
const PIXEL_FIELDS: &str = "id,name,creation_time,last_fired_time,is_unavailable";

#[derive(Debug, clap::Subcommand)]
pub enum ObjectCommand {
  /// Campaigns of an ad account
  Campaigns {
    #[command(flatten)]
    scope: Scope,
  },
  /// Ad sets, optionally of one campaign
  Adsets {
    #[command(flatten)]
    scope: Scope,
    /// Only the ad sets of this campaign
    #[arg(long, value_name = "ID")]
    campaign: Option<String>,
  },
  /// Ads, optionally of one campaign or ad set
  Ads {
    #[command(flatten)]
    scope: Scope,
    /// Only the ads of this campaign
    #[arg(long, value_name = "ID")]
    campaign: Option<String>,
    /// Only the ads of this ad set (wins over --campaign)
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
    /// Object id
    id: String,
    /// Comma-separated fields
    #[arg(long)]
    fields: Option<String>,
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

pub async fn run(ctx: &Ctx, graph: &Graph<'_>, command: ObjectCommand) -> Result<Data> {
  let list = |scope, edge, names, parent| list(ctx, graph, scope, edge, names, parent);
  match command {
    ObjectCommand::Campaigns { scope } => list(scope, "campaigns", CAMPAIGN_FIELDS, None).await,
    ObjectCommand::Adsets { scope, campaign } => {
      let parent = campaign.map(|c| format!("{c}/adsets"));
      list(scope, "adsets", ADSET_FIELDS, parent).await
    }
    ObjectCommand::Ads {
      scope,
      campaign,
      adset,
    } => {
      let parent = adset.or(campaign).map(|p| format!("{p}/ads"));
      list(scope, "ads", AD_FIELDS, parent).await
    }
    ObjectCommand::Creatives { scope } => list(scope, "adcreatives", CREATIVE_FIELDS, None).await,
    ObjectCommand::Audiences { scope } => {
      list(scope, "customaudiences", AUDIENCE_FIELDS, None).await
    }
    ObjectCommand::Pixels { scope } => list(scope, "adspixels", PIXEL_FIELDS, None).await,
    ObjectCommand::Get { id, fields: names } => {
      let query = names.map(|f| fields(&f)).unwrap_or_default();
      Ok(Data::Value(graph.get(&id, &query).await?))
    }
  }
}

/// One edge of the ad account (or of `parent`), as rows.
async fn list(
  ctx: &Ctx,
  graph: &Graph<'_>,
  scope: Scope,
  edge: &str,
  names: &str,
  parent: Option<String>,
) -> Result<Data> {
  let path = match parent {
    Some(p) => p,
    None => format!("{}/{edge}", account(ctx, scope.account.as_deref())?),
  };
  let mut query: Params = vec![(
    "fields".into(),
    scope.fields.unwrap_or_else(|| names.to_owned()),
  )];
  if !scope.status.is_empty() {
    let statuses: Vec<String> = scope.status.iter().map(|s| s.to_uppercase()).collect();
    query.push(("effective_status".into(), json!(statuses).to_string()));
  }
  Ok(rows(graph.all(&path, &query, scope.limit).await?))
}

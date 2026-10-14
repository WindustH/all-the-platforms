//! Changes: new campaigns, field updates, pausing, activating, deleting.

use atp_core::cli::confirm;
use atp_core::{Action, Ctx, Data, Error, Result, ValueExt, json};

use super::{account, key_values};
use crate::graph::{Graph, Params};

#[derive(Debug, clap::Subcommand)]
pub enum ChangeCommand {
  /// Create a campaign (paused unless --status ACTIVE)
  #[command(after_long_help = EX_CREATE_CAMPAIGN)]
  CreateCampaign(NewCampaign),
  /// Change fields of an object: `ads update ID name=Spring daily_budget=5000`
  Update {
    /// Campaign, ad set, ad or other object id
    id: String,
    /// key=value pairs
    #[arg(required = true)]
    fields: Vec<String>,
  },
  /// Pause campaigns, ad sets or ads
  Pause {
    /// Campaign, ad set or ad ids
    ids: Vec<String>,
  },
  /// Turn campaigns, ad sets or ads on
  Activate {
    /// Campaign, ad set or ad ids
    ids: Vec<String>,
  },
  /// Delete campaigns, ad sets, ads, creatives or audiences
  Delete {
    /// Ids of campaigns, ad sets, ads, creatives or audiences
    ids: Vec<String>,
    /// Skip the confirmation question (needed in scripts)
    #[arg(short, long)]
    yes: bool,
  },
}

#[derive(Debug, clap::Args)]
pub struct NewCampaign {
  /// Ad account id (`act_` optional; default: META_AD_ACCOUNT_ID or `ads use`)
  #[arg(short, long, value_name = "ID")]
  account: Option<String>,
  /// Campaign name
  #[arg(long)]
  name: String,
  /// OUTCOME_AWARENESS, OUTCOME_TRAFFIC, OUTCOME_ENGAGEMENT, OUTCOME_LEADS,
  /// OUTCOME_APP_PROMOTION or OUTCOME_SALES
  #[arg(long)]
  objective: String,
  /// Status to start with: PAUSED or ACTIVE
  #[arg(long, default_value = "PAUSED")]
  status: String,
  /// Daily budget in the account currency's minor unit (cents)
  #[arg(long)]
  daily_budget: Option<u64>,
  /// Lifetime budget in the account currency's minor unit (cents)
  #[arg(long)]
  lifetime_budget: Option<u64>,
  /// HOUSING, EMPLOYMENT, CREDIT, ISSUES_ELECTIONS_POLITICS ... (repeatable; none by default)
  #[arg(long = "special-ad-category")]
  special: Vec<String>,
}

const EX_CREATE_CAMPAIGN: &str = "\
Examples:
  atp fb ads create-campaign --name 'Spring sale' --objective OUTCOME_TRAFFIC --daily-budget 5000
  atp fb ads create-campaign -a 1234567890 --name Leads --objective outcome_leads \\
    --special-ad-category employment
  atp fb ads activate 120211234567890

More: atp guide facebook ads";

pub async fn run(ctx: &Ctx, graph: &Graph<'_>, command: ChangeCommand) -> Result<Data> {
  match command {
    ChangeCommand::CreateCampaign(new) => create_campaign(ctx, graph, new).await,
    ChangeCommand::Update { id, fields } => {
      graph.post(&id, &key_values(&fields)?).await?;
      Ok(Data::Action(Action::done("update", &id).with_id(id)))
    }
    ChangeCommand::Pause { ids } => set_status(graph, &ids, "PAUSED").await,
    ChangeCommand::Activate { ids } => set_status(graph, &ids, "ACTIVE").await,
    ChangeCommand::Delete { ids, yes } => {
      confirm(yes, &format!("delete {}", ids.join(", ")))?;
      for id in &ids {
        graph.delete(id).await?;
      }
      Ok(Data::Action(Action::done("delete", ids.join(", "))))
    }
  }
}

async fn create_campaign(ctx: &Ctx, graph: &Graph<'_>, new: NewCampaign) -> Result<Data> {
  let account = account(ctx, new.account.as_deref())?;
  let categories: Vec<String> = new.special.iter().map(|s| s.to_uppercase()).collect();
  let mut form: Params = vec![
    ("name".into(), new.name.clone()),
    ("objective".into(), new.objective.to_uppercase()),
    ("status".into(), new.status.to_uppercase()),
    (
      "special_ad_categories".into(),
      json!(categories).to_string(),
    ),
  ];
  if let Some(budget) = new.daily_budget {
    form.push(("daily_budget".into(), budget.to_string()));
  }
  if let Some(budget) = new.lifetime_budget {
    form.push(("lifetime_budget".into(), budget.to_string()));
  }
  let v = graph.post(&format!("{account}/campaigns"), &form).await?;
  Ok(Data::Action(
    Action::done("create-campaign", new.name)
      .with_id(v.str("id").unwrap_or_default())
      .with_message(format!("status {}", new.status)),
  ))
}

async fn set_status(graph: &Graph<'_>, ids: &[String], status: &str) -> Result<Data> {
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

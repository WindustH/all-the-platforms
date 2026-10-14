//! Who the token is, the ad accounts it can use, and the default one.

use atp_core::{Action, Ctx, Data, Result, ValueExt};

use super::{ACCOUNT_KEY, account, rows};
use crate::graph::{self, Graph, fields};

const ACCOUNT_FIELDS: &str = "id,account_id,name,account_status,currency,timezone_name,amount_spent,balance,spend_cap,business{id,name}";

#[derive(Debug, clap::Subcommand)]
pub enum AccountCommand {
  /// The person or system user the access token belongs to
  Me,
  /// Ad accounts the token can use
  Accounts {
    /// How many accounts (0 = all)
    #[arg(short = 'n', long, default_value_t = 100)]
    limit: usize,
  },
  /// Details of ad accounts (default: the current one)
  Account {
    /// Ad account ids (`act_` optional)
    ids: Vec<String>,
  },
  /// Remember the ad account to use by default
  Use {
    /// Ad account id (`act_` optional)
    account: String,
  },
}

pub async fn run(ctx: &Ctx, graph: &Graph<'_>, command: AccountCommand) -> Result<Data> {
  match command {
    AccountCommand::Me => Ok(Data::User(Box::new(graph::me(graph, "id,name").await?))),
    AccountCommand::Accounts { limit } => {
      let items = graph
        .all("me/adaccounts", &fields(ACCOUNT_FIELDS), limit)
        .await?;
      Ok(rows(items))
    }
    AccountCommand::Account { ids } => details(ctx, graph, &ids).await,
    AccountCommand::Use { account: id } => use_account(ctx, graph, &id).await,
  }
}

async fn details(ctx: &Ctx, graph: &Graph<'_>, ids: &[String]) -> Result<Data> {
  let ids = if ids.is_empty() {
    vec![account(ctx, None)?]
  } else {
    ids
      .iter()
      .map(|id| account(ctx, Some(id)))
      .collect::<Result<_>>()?
  };
  let mut out = Vec::new();
  for id in ids {
    out.push(graph.get(&id, &fields(ACCOUNT_FIELDS)).await?);
  }
  Ok(rows(out))
}

async fn use_account(ctx: &Ctx, graph: &Graph<'_>, given: &str) -> Result<Data> {
  let id = account(ctx, Some(given))?;
  let v = graph.get(&id, &fields("id,name")).await?;
  ctx.set_extra(ACCOUNT_KEY, &id);
  Ok(Data::Action(
    Action::done("use", v.str("name").unwrap_or_else(|| id.clone()))
      .with_id(id)
      .with_message("used by default from now on"),
  ))
}

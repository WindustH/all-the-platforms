//! The Marketing API (`atp facebook ads ...`, `atp instagram ads ...`):
//! ad accounts, campaigns, ad sets, ads, creatives, audiences, pixels, and
//! insights reports as flat rows, plus `raw` for any other endpoint. It
//! covers what Meta's own Ads CLI does, over the same Graph API.
//!
//! The ad account: `-a`, `META_AD_ACCOUNT_ID`, or the one saved by `ads use`.
//! New campaigns start paused; `activate` turns them on.

mod accounts;
mod changes;
mod objects;
mod raw;
mod reports;

use atp_core::{Ctx, Data, Error, Result, Value, credentials};

use crate::graph::{Graph, Params};

/// Session extra holding the ad account chosen with `ads use`.
const ACCOUNT_KEY: &str = "ad_account";

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  #[command(flatten)]
  Accounts(accounts::AccountCommand),
  #[command(flatten)]
  Objects(objects::ObjectCommand),
  #[command(flatten)]
  Changes(changes::ChangeCommand),
  #[command(flatten)]
  Reports(reports::ReportCommand),
  #[command(flatten)]
  Raw(raw::RawCommand),
}

pub async fn run(ctx: &Ctx, graph: &Graph<'_>, command: Command) -> Result<Data> {
  match command {
    Command::Accounts(command) => accounts::run(ctx, graph, command).await,
    Command::Objects(command) => objects::run(ctx, graph, command).await,
    Command::Changes(command) => changes::run(ctx, graph, command).await,
    Command::Reports(command) => reports::run(ctx, graph, command).await,
    Command::Raw(command) => raw::run(graph, command).await,
  }
}

/// `act_<id>` from `-a`, `META_AD_ACCOUNT_ID` or the saved choice.
fn account(ctx: &Ctx, given: Option<&str>) -> Result<String> {
  let id = given
    .map(str::to_owned)
    .or_else(|| credentials::env("META_AD_ACCOUNT_ID"))
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

/// `key=value` arguments as parameters.
fn key_values(pairs: &[String]) -> Result<Params> {
  pairs
    .iter()
    .map(|pair| {
      pair
        .split_once('=')
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .ok_or_else(|| Error::input(format!("expected key=value, got `{pair}`")))
    })
    .collect()
}

fn rows(items: Vec<Value>) -> Data {
  Data::Value(Value::Array(items))
}

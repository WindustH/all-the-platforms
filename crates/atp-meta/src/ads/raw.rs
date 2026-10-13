//! Any Graph endpoint, for what the other commands do not cover.

use atp_core::{Data, Error, Result};

use super::key_values;
use crate::graph::Graph;

#[derive(Debug, clap::Subcommand)]
pub enum RawCommand {
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

pub async fn run(graph: &Graph<'_>, command: RawCommand) -> Result<Data> {
  let RawCommand::Raw {
    path,
    params,
    method,
  } = command;
  let params = key_values(&params)?;
  let v = match method.to_uppercase().as_str() {
    "GET" => graph.get(&path, &params).await?,
    "POST" => graph.post(&path, &params).await?,
    "DELETE" => graph.delete(&path).await?,
    other => return Err(Error::input(format!("unknown method {other}"))),
  };
  Ok(Data::Value(v))
}

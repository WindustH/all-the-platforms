//! Background tasks: moves, copies, deletes, shares and saves answer with a
//! `task_id` that `/task` reports on (`status` 2 done, 3 failed); the
//! answer's `metadata.tq_gap` says when to look first.

use std::time::Duration;

use atp_core::task::{self, Progress};
use atp_core::{Error, Result, Value, ValueExt};

use crate::api::Api;

const FIRST: Duration = Duration::from_millis(500);
const MAX_GAP: Duration = Duration::from_secs(3);
const TIMEOUT: Duration = Duration::from_secs(180);

/// The task a write answered with, waited for (even one marked `finish`,
/// whose results only `/task` tells); its finished `data`, or the answer's
/// own `data` when there is no task.
pub async fn settle(api: &Api, answer: &Value) -> Result<Value> {
  let data = answer.at("data");
  let Some(id) = data.str("task_id") else {
    return Ok(data.clone());
  };
  let first = answer
    .u64("metadata.tq_gap")
    .map(Duration::from_millis)
    .unwrap_or(FIRST)
    .clamp(Duration::from_millis(200), MAX_GAP);
  wait(api, &id, first).await
}

/// Look at task `id` until it is done.
pub async fn wait(api: &Api, id: &str, first: Duration) -> Result<Value> {
  task::wait(first, MAX_GAP, TIMEOUT, async |round| {
    let query = [
      ("task_id", id.to_owned()),
      ("retry_index", round.to_string()),
    ];
    let v = api.get("/task", &query).await?;
    let data = v.at("data");
    match data.i64("status") {
      Some(2) => Ok(Progress::Done(data.clone())),
      Some(3) => Err(failed(data)),
      _ => Ok(Progress::Running),
    }
  })
  .await
}

fn failed(data: &Value) -> Error {
  let title = data.str("task_title").unwrap_or_else(|| "the task".into());
  let reason = data
    .first_str(&["message", "err_msg", "task_resp.message"])
    .unwrap_or_else(|| "no reason given".into());
  Error::upstream(format!("Quark: {title} failed: {reason}"))
}

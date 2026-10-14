//! Background tasks of the web drive: file-manager changes and saves of
//! shared files answer with a task id (`taskid`, `task_id`) that
//! `/share/taskquery` reports on (`status` success, failed or running).

use std::time::Duration;

use atp_core::task::{self, Progress};
use atp_core::{Error, Result, Value, ValueExt};

use crate::api::{Api, Route, errno, refusal};

const FIRST: Duration = Duration::from_millis(500);
const MAX_GAP: Duration = Duration::from_secs(5);
const TIMEOUT: Duration = Duration::from_secs(300);

/// The task an answer started, waited for; the answer itself when it
/// finished at once.
pub async fn settle(api: &Api, answer: &Value) -> Result<Value> {
  let id = answer
    .first_str(&["taskid", "task_id"])
    .filter(|id| id != "0");
  let Some(id) = id else {
    items_done(answer, "change", Route::Web)?;
    return Ok(answer.clone());
  };
  wait(api, &id).await
}

/// Look at task `id` until it is done.
pub async fn wait(api: &Api, id: &str) -> Result<Value> {
  task::wait(FIRST, MAX_GAP, TIMEOUT, async |_| {
    let v = api
      .get("/share/taskquery", &[("taskid", id.to_owned())])
      .await?;
    match v.str("status").as_deref() {
      Some("success") => Ok(Progress::Done(v)),
      Some("failed") => Err(failed(&v)),
      _ => Ok(Progress::Running),
    }
  })
  .await
}

/// Fail on the first entry of `info` (or `list`) that was refused.
pub fn items_done(v: &Value, what: &str, route: Route) -> Result<()> {
  for item in v.list("info").iter().chain(v.list("list")) {
    let code = item.i64("errno").unwrap_or(0);
    if code != 0 {
      let path = item.str("path").unwrap_or_default();
      return Err(refusal(item, code, &format!("{what} {path}"), route));
    }
  }
  Ok(())
}

fn failed(v: &Value) -> Error {
  if let Err(e) = items_done(v, "task", Route::Web) {
    return e;
  }
  let code = v
    .i64("task_errno")
    .filter(|c| *c != 0)
    .unwrap_or_else(|| errno(v));
  refusal(v, code, "background task", Route::Web)
}

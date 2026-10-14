//! Work an upstream finishes in the background (moves, copies, deletes,
//! saves of shared files, transfers), and calls a rate limit or risk check
//! turns away for a while.

use std::time::{Duration, Instant};

use crate::error::{Error, Result};
use crate::live::Backoff;

/// What one look at a background task found.
pub enum Progress<T> {
  Running,
  /// Finished, with what the task produced.
  Done(T),
}

/// Look at a task with `check(round)` (rounds count from 0) until it is
/// done: first after `first`, then in gaps doubling up to `max`, for at
/// most `timeout`. `check` fails when the task failed.
pub async fn wait<T>(
  first: Duration,
  max: Duration,
  timeout: Duration,
  mut check: impl AsyncFnMut(u32) -> Result<Progress<T>>,
) -> Result<T> {
  let started = Instant::now();
  let mut backoff = Backoff::new(first, max);
  let mut round = 0;
  loop {
    backoff.wait().await;
    if let Progress::Done(value) = check(round).await? {
      return Ok(value);
    }
    if started.elapsed() >= timeout {
      return Err(
        Error::upstream(format!(
          "the task is still running after {} s",
          timeout.as_secs()
        ))
        .with_hint("it may still finish upstream; check before running it again"),
      );
    }
    round += 1;
  }
}

/// Run `call`, and again after a pause while it fails with an error
/// `risky` picks (a rate limit, a "too frequent" or risk-control code), at
/// most `tries` calls in all. Pauses start at 3 s and double up to 20 s.
pub async fn retry<T>(
  tries: u32,
  risky: impl Fn(&Error) -> bool,
  mut call: impl AsyncFnMut() -> Result<T>,
) -> Result<T> {
  let mut backoff = Backoff::new(Duration::from_secs(3), Duration::from_secs(20));
  let mut left = tries.max(1);
  loop {
    left -= 1;
    match call().await {
      Err(e) if left > 0 && risky(&e) => {
        tracing::debug!("{}; trying again", e.message);
        backoff.wait().await;
      }
      result => return result,
    }
  }
}

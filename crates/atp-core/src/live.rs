//! Building blocks of `listen`: polling for platforms without a push
//! channel, and reconnect delays for socket listeners.

use std::collections::{HashSet, VecDeque};
use std::time::Duration;

use crate::error::Result;
use crate::model::Post;
use crate::output::Stream;

/// How many recent ids [`Seen`] remembers.
const SEEN_CAPACITY: usize = 5000;

/// Ids already printed, so polled items repeat nowhere.
#[derive(Default)]
pub struct Seen {
  set: HashSet<String>,
  order: VecDeque<String>,
}

impl Seen {
  /// Record `id`; true when it was new.
  pub fn insert(&mut self, id: &str) -> bool {
    if !self.set.insert(id.to_owned()) {
      return false;
    }
    self.order.push_back(id.to_owned());
    if self.order.len() > SEEN_CAPACITY
      && let Some(old) = self.order.pop_front()
    {
      self.set.remove(&old);
    }
    true
  }
}

/// Poll every `every`: `fetch` returns the newest items (any order). The
/// first round only records what already exists; later rounds print the
/// unseen items oldest first. Runs until `fetch` fails or the user stops it.
pub async fn poll(
  out: &Stream,
  every: Duration,
  mut fetch: impl AsyncFnMut() -> Result<Vec<Post>>,
) -> Result<()> {
  let mut seen = Seen::default();
  let mut first = true;
  loop {
    let mut items = fetch().await?;
    items.sort_by_key(|p| p.created_at);
    for post in &items {
      if seen.insert(&post.id) && !first {
        out.post(post);
      }
    }
    first = false;
    tokio::time::sleep(every).await;
  }
}

/// Delays between reconnects of a socket listener: 1 s doubling to 60 s,
/// back to 1 s once a connection stayed up a while.
pub struct Backoff {
  next: Duration,
}

impl Default for Backoff {
  fn default() -> Self {
    Self {
      next: Duration::from_secs(1),
    }
  }
}

impl Backoff {
  /// Wait before the next attempt.
  pub async fn wait(&mut self) {
    tokio::time::sleep(self.next).await;
    self.next = (self.next * 2).min(Duration::from_secs(60));
  }

  /// The connection worked: start over at 1 s.
  pub fn reset(&mut self) {
    self.next = Duration::from_secs(1);
  }
}

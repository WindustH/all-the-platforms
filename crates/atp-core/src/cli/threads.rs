//! `comments --replies`: the full reply threads of a listing's comments,
//! within its time window and page bound.

use crate::error::Result;
use crate::model::Comment;
use crate::paging::{Bounds, Window, collect_within};
use crate::platform::{Cap, Platform};

/// How deep `comments --replies` follows nested threads.
const REPLY_DEPTH: usize = 3;

/// What `comments --replies` fetches the threads of a post with: the
/// listing's `--since` / `--until` and `--max-pages`.
pub(super) struct Threads<'a> {
  pub post: &'a str,
  pub window: Window,
  pub bounds: Bounds,
}

impl Threads<'_> {
  /// Fetch the full reply thread of every comment whose inline replies are
  /// incomplete (platforms usually inline only a few), down to
  /// [`REPLY_DEPTH`], and keep the replies inside the time window. A thread
  /// cut short (`--max-pages`) keeps its cursor in `replies_cursor`.
  pub async fn complete<P: Platform>(
    &self,
    p: &P,
    comments: &mut [Comment],
    depth: usize,
  ) -> Result<()> {
    let fetch = P::INFO.supports(Cap::Replies) && depth < REPLY_DEPTH;
    for c in comments.iter_mut() {
      let expected = c.reply_count.unwrap_or(0) as usize;
      if fetch && expected > 0 && c.replies.len() < expected {
        let id = c.id.clone();
        let thread = collect_within(self.bounds, None, async |r| {
          p.replies(self.post, &id, &r).await
        })
        .await?;
        c.replies = thread.items;
        c.replies_cursor = thread.next_cursor;
      }
      c.replies.retain(|r| self.window.contains(r.created_at));
      Box::pin(self.complete(p, &mut c.replies, depth + 1)).await?;
    }
    Ok(())
  }

  /// Say how many threads stopped before their end.
  pub fn report(&self, comments: &[Comment]) {
    fn cut(comments: &[Comment]) -> usize {
      comments
        .iter()
        .map(|c| usize::from(c.replies_cursor.is_some()) + cut(&c.replies))
        .sum()
    }
    let n = cut(comments);
    if n > 0 {
      crate::output::note(&format!(
        "{n} of the reply threads stopped before their end; `replies_cursor` continues each"
      ));
    }
  }
}

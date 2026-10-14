//! Entries list with their own links, which is what `#N` hands back; the
//! index remembers which entry each listed link was (`FEED_URL#ID` or
//! `miniflux:<id>`), so `read`, `favorite` and `mark` take links too.

use std::time::Duration;

use atp_core::{Ctx, Post, Value};

const KEY: &str = "entries";
/// How many links are remembered.
const KEEP: usize = 2000;
/// How long they are.
const TTL: Duration = Duration::from_secs(30 * 86_400);
/// The `extra` key holding an entry's own reference.
pub const REF: &str = "ref";

/// Remember the links of listed entries.
pub fn remember(ctx: &Ctx, posts: &[Post]) {
  let mut pairs: Vec<(String, String)> = ctx.store.cache_get(KEY, TTL).unwrap_or_default();
  for post in posts {
    let (Some(link), Some(entry)) = (&post.url, post.extra.get(REF).and_then(Value::as_str)) else {
      continue;
    };
    if link != entry {
      pairs.retain(|(known, _)| known != link);
      pairs.push((link.clone(), entry.to_owned()));
    }
  }
  let start = pairs.len().saturating_sub(KEEP);
  ctx.store.cache_put(KEY, &pairs[start..].to_vec());
}

/// The entry a listed link was.
pub fn lookup(ctx: &Ctx, link: &str) -> Option<String> {
  let pairs: Vec<(String, String)> = ctx.store.cache_get(KEY, TTL)?;
  pairs
    .into_iter()
    .rev()
    .find(|(known, _)| known == link)
    .map(|(_, entry)| entry)
}

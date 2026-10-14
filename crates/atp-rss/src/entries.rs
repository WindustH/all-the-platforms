//! Reading: one entry or a feed's overview, a feed's entries, the feed
//! as a source, the timeline (local subscriptions or the Miniflux
//! account), search, and starring and marking entries on Miniflux.

use atp_core::paging::slice;
use atp_core::{Action, Error, Page, PageReq, Post, Query, Result, User};

use crate::api::Api;
use crate::refs::{self, Entry};
use crate::{index, parse, subscriptions};

/// `feed --type`: the local subscriptions, or the Miniflux account's
/// unread, all or starred entries.
pub const FEEDS: &[&str] = &["local", "unread", "all", "starred"];

/// An entry, or a feed with its latest entries.
pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let entry =
    refs::entry(arg).or_else(|| index::lookup(&api.ctx, arg).and_then(|r| refs::entry(&r)));
  match entry {
    Some(Entry::Miniflux(id)) => api.require_miniflux()?.entry(id).await,
    Some(Entry::Feed { feed, id }) => {
      let loaded = api.load(&feed).await?;
      let entry = loaded
        .entry(&id)
        .ok_or_else(|| Error::not_found(format!("the feed {feed} no longer carries entry {id}")))?;
      Ok(parse::entry(&loaded, entry))
    }
    None => Ok(parse::overview(&api.load(arg).await?)),
  }
}

/// A feed as the source of its entries.
pub async fn source(api: &Api, arg: &str) -> Result<User> {
  Ok(parse::source(&api.load(arg).await?))
}

/// A feed's entries, the newest first.
pub async fn posts(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let feed = api.load(arg).await?;
  let mut items: Vec<Post> = feed
    .entries
    .iter()
    .map(|e| parse::entry(&feed, e))
    .collect();
  items.sort_by_key(|post| std::cmp::Reverse(post.created_at));
  let page = slice(items, page);
  index::remember(&api.ctx, &page.items);
  Ok(page)
}

/// The timeline: Miniflux's unread entries when an account is logged in,
/// else the local subscriptions merged.
pub async fn feed(api: &Api, kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let default = if api.miniflux().is_some() {
    "unread"
  } else {
    "local"
  };
  let page = match kind.unwrap_or(default) {
    "local" => return subscriptions::merged(api, page).await,
    kind => api.require_miniflux()?.listing(kind, page).await?,
  };
  index::remember(&api.ctx, &page.items);
  Ok(page)
}

/// Search the Miniflux account, or the entries of the local subscriptions
/// (titles and text, ignoring case).
pub async fn search(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  let local = query.filter.as_deref() == Some("local") || api.miniflux().is_none();
  let page = if local {
    let words = query.keyword.to_lowercase();
    let found: Vec<Post> = subscriptions::timeline(api)
      .await?
      .into_iter()
      .filter(|post| {
        let title = post.title.as_deref().unwrap_or_default().to_lowercase();
        let text = post.text.as_deref().unwrap_or_default().to_lowercase();
        title.contains(&words) || text.contains(&words)
      })
      .collect();
    slice(found, page)
  } else {
    let filters = vec![("search", query.keyword.clone())];
    api.require_miniflux()?.entries(filters, page).await?
  };
  index::remember(&api.ctx, &page.items);
  Ok(page)
}

/// A Miniflux entry argument: `miniflux:<id>`, the id, or a listed link.
fn miniflux_entry(api: &Api, arg: &str) -> Result<u64> {
  refs::miniflux_id(arg)
    .or_else(|| index::lookup(&api.ctx, arg).and_then(|r| refs::miniflux_id(&r)))
    .ok_or_else(|| {
      Error::input(format!("not a Miniflux entry: {arg}"))
        .with_hint("star and mark entries listed from the Miniflux account (`feed`, `favorites`)")
    })
}

pub async fn star(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let miniflux = api.require_miniflux()?;
  miniflux.star(miniflux_entry(api, arg)?, undo).await
}

pub async fn starred(
  api: &Api,
  user: Option<&str>,
  folder: Option<&str>,
  page: &PageReq,
) -> Result<Page<Post>> {
  if user.is_some() || folder.is_some() {
    return Err(
      Error::unsupported("favorites USER / --folder")
        .with_hint("Miniflux shows your own starred entries, without folders"),
    );
  }
  let page = api.require_miniflux()?.listing("starred", page).await?;
  index::remember(&api.ctx, &page.items);
  Ok(page)
}

/// Mark entries read or unread.
pub async fn mark(api: &Api, args: &[String], unread: bool) -> Result<Action> {
  let miniflux = api.require_miniflux()?;
  let ids = args
    .iter()
    .map(|arg| miniflux_entry(api, &api.ctx.post_ref(arg)?))
    .collect::<Result<Vec<u64>>>()?;
  miniflux.mark(&ids, !unread).await?;
  let action = if unread { "mark-unread" } else { "mark-read" };
  Ok(Action::done(action, args.join(" ")).with_message(format!("{} entries", ids.len())))
}

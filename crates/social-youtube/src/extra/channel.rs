//! A channel's other tabs (Shorts, live streams, community posts, releases,
//! podcasts), the search inside it, and RSS feeds with exact times.

use social_core::cli::PageArgs;
use social_core::{Data, Page, Result};

use crate::api::Api;
use crate::channels::{self, Tab};
use crate::rss;

/// A tab of posts (videos, Shorts, streams, community posts).
pub async fn tab(api: &Api, user: &str, tab: Tab, page: &PageArgs) -> Result<Data> {
  let user = api.ctx.user_ref(user)?;
  let posts = page.collect_dated(async |r| {
    let listing = channels::tab(api, &user, tab, &r).await?;
    Ok(Page::new(listing.posts, listing.next))
  });
  Ok(Data::Posts(posts.await?))
}

/// A tab of shelves (releases, podcasts): playlists.
pub async fn shelves(api: &Api, user: &str, tab: Tab, page: &PageArgs) -> Result<Data> {
  let user = api.ctx.user_ref(user)?;
  let shelves = page.collect(async |r| {
    let listing = channels::tab(api, &user, tab, &r).await?;
    Ok(Page::new(listing.collections, listing.next))
  });
  Ok(Data::Collections(shelves.await?))
}

pub async fn search(api: &Api, user: &str, query: &str, page: &PageArgs) -> Result<Data> {
  let user = api.ctx.user_ref(user)?;
  let posts = page.collect(async |r| {
    let listing = channels::search(api, &user, query, &r).await?;
    Ok(Page::new(listing.posts, listing.next))
  });
  Ok(Data::Posts(posts.await?))
}

/// The RSS feed of a channel, or of a playlist when `playlist`.
pub async fn rss(api: &Api, target: &str, playlist: bool) -> Result<Data> {
  let target = if playlist {
    target.to_owned()
  } else {
    api.ctx.user_ref(target)?
  };
  Ok(Data::Posts(Page::last(
    rss::feed(api, &target, playlist).await?,
  )))
}

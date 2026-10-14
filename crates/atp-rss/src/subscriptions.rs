//! The local subscription list: feeds kept in a private data file
//! (`feeds.json` beside the session, so `logout` leaves it alone), and
//! the merged timeline of all of them.

use atp_core::output::note;
use atp_core::paging::slice;
use atp_core::{Action, Error, Page, PageReq, Post, Result, User, feed};
use futures_util::{StreamExt, stream};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::api::Api;
use crate::refs::HUB_SCHEME;
use crate::{index, parse};

const DATA: &str = "feeds";
/// Feeds fetched at once for the merged timeline.
const CONCURRENCY: usize = 4;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subscription {
  /// The feed's URL, or an RSSHub route (`rsshub:/…`).
  pub url: String,
  #[serde(default)]
  pub title: Option<String>,
  #[serde(default)]
  pub added: Option<Timestamp>,
}

pub fn load(api: &Api) -> Result<Vec<Subscription>> {
  Ok(api.ctx.store.data(DATA)?.unwrap_or_default())
}

fn save(api: &Api, subscriptions: &[Subscription]) -> Result<()> {
  api.ctx.store.put_data(DATA, &subscriptions)
}

/// Whether a subscription is the feed an argument names.
fn matches(api: &Api, subscription: &Subscription, arg: &str) -> bool {
  let arg = arg.trim();
  subscription.url == arg
    || api.feed_url(&subscription.url).ok().as_deref() == Some(arg)
    || api.feed_url(arg).ok().as_deref() == Some(subscription.url.as_str())
}

/// Subscribe: the feed is fetched once to check it (and to find it when
/// given a web page); RSSHub routes are kept as routes.
pub async fn add(api: &Api, arg: &str, title: Option<String>) -> Result<Action> {
  let feed = api.load(arg).await?;
  let url = if arg.trim().starts_with(HUB_SCHEME) {
    arg.trim().to_owned()
  } else {
    feed.url.clone()
  };
  let mut subscriptions = load(api)?;
  if subscriptions.iter().any(|s| matches(api, s, &url)) {
    return Ok(Action::done("add", url).with_message("already subscribed"));
  }
  let title = title.or(feed.title.clone());
  subscriptions.push(Subscription {
    url: url.clone(),
    title: title.clone(),
    added: Some(Timestamp::now()),
  });
  save(api, &subscriptions)?;
  let mut action = Action::done("add", url);
  if let Some(title) = title {
    action = action.with_message(title);
  }
  Ok(action)
}

pub fn remove(api: &Api, arg: &str) -> Result<Action> {
  let mut subscriptions = load(api)?;
  let before = subscriptions.len();
  subscriptions.retain(|s| !matches(api, s, arg));
  if subscriptions.len() == before {
    return Err(
      Error::not_found(format!("no subscription {arg}"))
        .with_hint("`atp rss list` shows the subscriptions"),
    );
  }
  save(api, &subscriptions)?;
  Ok(Action::done("remove", arg))
}

/// The subscriptions as sources: the feed's URL as their link (routes on
/// the RSSHub instance in use), the stored form as their id.
pub fn list(api: &Api) -> Result<Vec<User>> {
  Ok(
    load(api)?
      .into_iter()
      .map(|s| {
        let mut user = User {
          name: s.title.clone().unwrap_or_else(|| s.url.clone()),
          url: api.feed_url(&s.url).ok(),
          id: s.url,
          ..User::default()
        };
        if let Some(added) = s.added {
          user.extra.insert("added_at".into(), atp_core::json!(added));
        }
        user
      })
      .collect(),
  )
}

/// Every entry of every subscription, the newest first. A feed that fails
/// is reported and skipped.
pub async fn timeline(api: &Api) -> Result<Vec<Post>> {
  let subscriptions = load(api)?;
  if subscriptions.is_empty() {
    return Err(
      Error::not_found("no subscriptions yet")
        .with_hint("add feeds with `atp rss add URL`, or log in to Miniflux"),
    );
  }
  let loaded: Vec<(String, Result<feed::Feed>)> = stream::iter(subscriptions)
    .map(|s| async move {
      let feed = api.load(&s.url).await;
      (s.url, feed)
    })
    .buffer_unordered(CONCURRENCY)
    .collect()
    .await;
  let mut posts = Vec::new();
  for (url, feed) in loaded {
    match feed {
      Ok(feed) => posts.extend(feed.entries.iter().map(|e| parse::entry(&feed, e))),
      Err(e) => note(&format!("skipped {url}: {}", e.message)),
    }
  }
  posts.sort_by_key(|post| std::cmp::Reverse(post.created_at));
  Ok(posts)
}

/// One page of the merged timeline.
pub async fn merged(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  let page = slice(timeline(api).await?, page);
  index::remember(&api.ctx, &page.items);
  Ok(page)
}

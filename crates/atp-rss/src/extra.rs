//! RSS commands beyond the shared set: the subscription list, RSSHub
//! routes, and marking Miniflux entries.

use atp_core::cli::{PageArgs, confirm};
use atp_core::{Data, Page, Result};

use crate::api::Api;
use crate::{entries, subscriptions};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Subscribe to a feed (kept by ATP, or on Miniflux with --remote)
  #[command(after_long_help = EX_ADD)]
  Add {
    /// Feed: URL, a site that announces its feed, or rsshub:/route
    feed: String,
    /// Your name for it (default: the feed's title)
    #[arg(long)]
    title: Option<String>,
    /// Subscribe the Miniflux account instead
    #[arg(long)]
    remote: bool,
    /// Miniflux category id (with --remote; default: the first one)
    #[arg(long)]
    category: Option<u64>,
  },
  /// Unsubscribe from a feed
  Remove {
    /// Feed: as `list` shows it, its URL, or #N after `list`
    feed: String,
    /// Unsubscribe the Miniflux account instead (by feed id or URL)
    #[arg(long)]
    remote: bool,
    /// Skip the confirmation question (needed in scripts)
    #[arg(short, long)]
    yes: bool,
  },
  /// Your subscriptions (or the Miniflux account's feeds with --remote)
  List {
    /// The Miniflux account's feeds, with unread counts
    #[arg(long)]
    remote: bool,
  },
  /// Entries of an RSSHub route
  #[command(after_long_help = EX_HUB)]
  Hub {
    /// Route, as RSSHub documents it (`/github/trending/daily`)
    route: String,
    /// RSSHub instance (default: RSSHUB_INSTANCE, else https://rsshub.app)
    #[arg(long)]
    instance: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Mark Miniflux entries read (or unread)
  Mark {
    /// Entries: miniflux:<id>, the id, a link from the last list, or #N
    #[arg(required = true)]
    entries: Vec<String>,
    /// Mark them unread instead
    #[arg(long)]
    unread: bool,
  },
}

const EX_ADD: &str = "\
Examples:
  atp rss add https://blog.rust-lang.org/feed.xml
  atp rss add https://lwn.net                     # finds the feed the page announces
  atp rss add rsshub:/github/trending/daily/rust --title 'Rust trending'
  atp rss add https://example.com/feed --remote   # on the Miniflux account

More: atp guide rss add";

const EX_HUB: &str = "\
Examples:
  atp rss hub /github/trending/daily/rust -n 10
  atp rss hub /bilibili/user/dynamic/2267573 --instance https://rsshub.example.org

Public instances limit or disable some routes; your own instance has none
of that. More: atp guide rss hub";

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    Command::Add {
      feed,
      title,
      remote,
      category,
    } => Data::Action(if remote {
      let url = api.feed_url(&feed)?;
      api.require_miniflux()?.subscribe(&url, category).await?
    } else {
      subscriptions::add(api, &feed, title).await?
    }),
    Command::Remove { feed, remote, yes } => {
      let feed = ctx.user_ref(&feed)?;
      Data::Action(if remote {
        let miniflux = api.require_miniflux()?;
        confirm(
          yes,
          &format!("unsubscribe the Miniflux account from {feed}"),
        )?;
        miniflux.unsubscribe(&feed).await?
      } else {
        subscriptions::remove(api, &feed)?
      })
    }
    Command::List { remote } => Data::Users(Page::last(if remote {
      api.require_miniflux()?.feeds().await?
    } else {
      subscriptions::list(api)?
    })),
    Command::Hub {
      route,
      instance,
      page,
    } => {
      let base = instance.unwrap_or_else(|| api.hub());
      let url = format!(
        "{}/{}",
        base.trim_end_matches('/'),
        route.trim_start_matches('/')
      );
      Data::Posts(
        page
          .collect_dated(async |req| entries::posts(api, &url, &req).await)
          .await?,
      )
    }
    Command::Mark { entries, unread } => Data::Action(entries::mark(api, &entries, unread).await?),
  })
}

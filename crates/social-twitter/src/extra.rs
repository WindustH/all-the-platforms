//! Twitter-only commands: retweets and quotes, profile tabs, lists,
//! communities, Spaces, trends by place, blocks and mutes, the pinned tweet,
//! scheduled posts and bookmark folders.

use std::path::PathBuf;

use social_core::cli::PageArgs;
use social_core::{Action, Data, Draft, Error, Page, Result};

use crate::api::Api;
use crate::community::{self, Sort};
use crate::tabs::{self, Tab};
use crate::{account, lists, spaces, tweets, users, write};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Retweet a tweet
  #[command(visible_alias = "repost")]
  Retweet {
    post: String,
    /// Undo the retweet instead
    #[arg(long)]
    undo: bool,
  },
  /// Quote a tweet with your own text
  Quote {
    post: String,
    text: String,
    /// Attach an image (repeatable, at most 4)
    #[arg(short = 'i', long = "image", value_name = "PATH")]
    images: Vec<PathBuf>,
  },
  /// Accounts that retweeted a tweet (its quotes: `reposts`)
  Retweeters {
    post: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// A profile tab: replies, media, highlights or articles
  Tab {
    user: String,
    #[arg(value_enum)]
    tab: Tab,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Verified followers of a user
  VerifiedFollowers {
    user: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Followers of a user that you follow too
  FollowersYouKnow {
    user: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Posts similar to a tweet
  Similar { post: String },
  /// Earlier versions of an edited tweet
  Edits { post: String },
  /// Latest tweets of a list (id or x.com/i/lists/... URL; see `collections`)
  List {
    list: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// A list's name, description, owner and counts
  ListInfo { list: String },
  /// Members of a list (with --followers, the accounts following it)
  ListMembers {
    list: String,
    #[arg(long)]
    followers: bool,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Lists a user (you, when omitted) is a member of
  ListMemberships {
    user: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Create a list
  ListCreate {
    name: String,
    #[arg(short, long)]
    description: Option<String>,
    #[arg(long)]
    private: bool,
  },
  /// Rename a list, change its description or privacy
  ListUpdate {
    list: String,
    #[arg(long)]
    name: Option<String>,
    #[arg(short, long)]
    description: Option<String>,
    #[arg(long)]
    private: Option<bool>,
  },
  /// Delete one of your lists
  ListDelete {
    list: String,
    #[arg(short, long)]
    yes: bool,
  },
  /// Add a user to a list (or remove them with --undo)
  ListAdd {
    list: String,
    user: String,
    #[arg(long)]
    undo: bool,
  },
  /// Follow a list (or unfollow it with --undo)
  ListFollow {
    list: String,
    #[arg(long)]
    undo: bool,
  },
  /// A community: description, rules, members, your role
  Community { community: String },
  /// Posts of a community
  CommunityPosts {
    community: String,
    #[arg(short, long, value_enum, default_value_t)]
    sort: Sort,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Members of a community (with --moderators, its moderators)
  CommunityMembers {
    community: String,
    #[arg(long)]
    moderators: bool,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Communities you belong to
  Communities,
  /// Join a community (or leave it with --undo)
  Join {
    community: String,
    #[arg(long)]
    undo: bool,
  },
  /// A Space: title, state, hosts, speakers and listeners
  Space { space: String },
  /// Posts sharing a Space that match a query (newest first; `extra.space` for `space`)
  Spaces {
    query: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Trends of a place (`trend-places` lists them; 1 is worldwide)
  Trends {
    #[arg(long, default_value = "1", value_name = "WOEID")]
    place: String,
  },
  /// Places with trends
  TrendPlaces,
  /// Block a user (or unblock with --undo)
  Block {
    user: String,
    #[arg(long)]
    undo: bool,
  },
  /// Mute a user (or unmute with --undo)
  Mute {
    user: String,
    #[arg(long)]
    undo: bool,
  },
  /// Accounts you block
  Blocked {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Accounts you mute
  Muted {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Pin a tweet to your profile (or unpin it with --undo)
  Pin {
    post: String,
    #[arg(long)]
    undo: bool,
  },
  /// Your scheduled posts
  Scheduled,
  /// Schedule a post: --at 2026-10-20T09:30:00+08:00, '2026-10-20 09:30' or +2h
  Schedule {
    text: String,
    #[arg(long, value_name = "WHEN")]
    at: String,
    #[arg(short = 'i', long = "image", value_name = "PATH")]
    images: Vec<PathBuf>,
  },
  /// Cancel a scheduled post
  Unschedule { id: String },
  /// Your bookmark folders (ids for `favorites --folder` and `favorite --folder`)
  Folders {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Create a bookmark folder
  FolderCreate { name: String },
  /// Rename a bookmark folder
  FolderRename { folder: String, name: String },
  /// Delete a bookmark folder (its bookmarks stay bookmarked)
  FolderDelete {
    folder: String,
    #[arg(short, long)]
    yes: bool,
  },
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  let ctx = &api.ctx;
  let post = |p: &str| ctx.post_ref(p);
  let user = |u: &str| ctx.user_ref(u);
  let coll = |c: &str| ctx.collection_ref(c);
  Ok(match command {
    Command::Retweet { post: p, undo } => {
      Data::Action(write::retweet(api, &post(&p)?, undo).await?)
    }
    Command::Quote {
      post: p,
      text,
      images,
    } => Data::Action(quote(api, &post(&p)?, &text, &images).await?),
    Command::Retweeters { post: p, page } => {
      let p = post(&p)?;
      Data::Users(
        page
          .collect(async |r| users::retweeters(api, &p, &r).await)
          .await?,
      )
    }
    Command::Tab { user: u, tab, page } => {
      let u = user(&u)?;
      Data::Posts(
        page
          .collect_dated(async |r| tabs::tab(api, &u, tab, &r).await)
          .await?,
      )
    }
    Command::VerifiedFollowers { user: u, page } => {
      let u = user(&u)?;
      Data::Users(
        page
          .collect(async |r| tabs::followers(api, &u, true, &r).await)
          .await?,
      )
    }
    Command::FollowersYouKnow { user: u, page } => {
      let u = user(&u)?;
      Data::Users(
        page
          .collect(async |r| tabs::followers(api, &u, false, &r).await)
          .await?,
      )
    }
    Command::Similar { post: p } => Data::Posts(tabs::similar(api, &post(&p)?).await?),
    Command::Edits { post: p } => Data::Posts(tabs::edits(api, &post(&p)?).await?),
    Command::List { list, page } => {
      let list = coll(&list)?;
      Data::Posts(
        page
          .collect_dated(async |r| tweets::list(api, &list, &r).await)
          .await?,
      )
    }
    Command::ListInfo { list } => one(lists::info(api, &coll(&list)?).await?),
    Command::ListMembers {
      list,
      followers,
      page,
    } => {
      let list = coll(&list)?;
      Data::Users(
        page
          .collect(async |r| lists::people(api, &list, followers, &r).await)
          .await?,
      )
    }
    Command::ListMemberships { user: u, page } => {
      let u = u.map(|u| user(&u)).transpose()?;
      let u = u.as_deref();
      Data::Collections(
        page
          .collect(async |r| lists::memberships(api, u, &r).await)
          .await?,
      )
    }
    Command::ListCreate {
      name,
      description,
      private,
    } => Data::Action(lists::create(api, &name, description.as_deref(), private).await?),
    Command::ListUpdate {
      list,
      name,
      description,
      private,
    } => Data::Action(
      lists::update(
        api,
        &coll(&list)?,
        name.as_deref(),
        description.as_deref(),
        private,
      )
      .await?,
    ),
    Command::ListDelete { list, yes } => {
      confirm(yes, "delete the list")?;
      Data::Action(lists::delete(api, &coll(&list)?).await?)
    }
    Command::ListAdd {
      list,
      user: u,
      undo,
    } => Data::Action(lists::member(api, &coll(&list)?, &user(&u)?, undo).await?),
    Command::ListFollow { list, undo } => {
      Data::Action(lists::follow(api, &coll(&list)?, undo).await?)
    }
    Command::Community { community } => one(community::info(api, &coll(&community)?).await?),
    Command::CommunityPosts {
      community,
      sort,
      page,
    } => {
      let c = coll(&community)?;
      Data::Posts(
        page
          .collect_dated(async |r| community::posts(api, &c, sort, &r).await)
          .await?,
      )
    }
    Command::CommunityMembers {
      community,
      moderators,
      page,
    } => {
      let c = coll(&community)?;
      Data::Users(
        page
          .collect(async |r| community::members(api, &c, moderators, &r).await)
          .await?,
      )
    }
    Command::Communities => Data::Collections(community::mine(api).await?),
    Command::Join { community, undo } => {
      Data::Action(community::join(api, &coll(&community)?, undo).await?)
    }
    Command::Space { space } => Data::Post(Box::new(spaces::space(api, &space).await?)),
    Command::Spaces { query, page } => Data::Posts(
      page
        .collect_dated(async |r| spaces::search(api, &query, &r).await)
        .await?,
    ),
    Command::Trends { place } => Data::Posts(account::place_trends(api, &place).await?),
    Command::TrendPlaces => Data::Collections(account::places(api).await?),
    Command::Block { user: u, undo } => {
      Data::Action(account::block(api, &user(&u)?, false, undo).await?)
    }
    Command::Mute { user: u, undo } => {
      Data::Action(account::block(api, &user(&u)?, true, undo).await?)
    }
    Command::Blocked { page } => Data::Users(
      page
        .collect(async |r| account::blocked(api, false, &r).await)
        .await?,
    ),
    Command::Muted { page } => Data::Users(
      page
        .collect(async |r| account::blocked(api, true, &r).await)
        .await?,
    ),
    Command::Pin { post: p, undo } => Data::Action(account::pin(api, &post(&p)?, undo).await?),
    Command::Scheduled => Data::Posts(account::scheduled(api).await?),
    Command::Schedule { text, at, images } => {
      let at = future_time(&at)?;
      social_core::cli::check_images(&images)?;
      let media = write::upload_images(api, &images).await?;
      Data::Action(account::schedule(api, &text, at, &media).await?)
    }
    Command::Unschedule { id } => Data::Action(account::unschedule(api, &id).await?),
    Command::Folders { page } => Data::Collections(
      page
        .collect(async |r| users::folders(api, &r).await)
        .await?,
    ),
    Command::FolderCreate { name } => one(account::create_folder(api, &name).await?),
    Command::FolderRename { folder, name } => {
      Data::Action(account::rename_folder(api, &coll(&folder)?, &name).await?)
    }
    Command::FolderDelete { folder, yes } => {
      confirm(yes, "delete the folder")?;
      Data::Action(account::delete_folder(api, &coll(&folder)?).await?)
    }
  })
}

fn one(c: social_core::Collection) -> Data {
  Data::Collections(Page::last(vec![c]))
}

fn confirm(yes: bool, what: &str) -> Result<()> {
  if yes {
    Ok(())
  } else {
    Err(Error::input(format!("pass --yes to {what}")))
  }
}

/// A time to schedule at: `+30m` / `+2h` / `+1d`, a local `YYYY-MM-DD HH:MM`,
/// or RFC 3339. Returns unix seconds, which must lie in the future.
fn future_time(s: &str) -> Result<i64> {
  let s = s.trim();
  let now = jiff::Timestamp::now();
  let bad = || Error::input(format!("bad offset `{s}` (use +30m, +2h or +1d)"));
  let at = if let Some(rel) = s.strip_prefix('+') {
    let (num, unit) = rel.split_at(rel.len().saturating_sub(1));
    let n: i64 = num.parse().map_err(|_| bad())?;
    let secs = match unit {
      "m" => 60,
      "h" => 3600,
      "d" => 86_400,
      _ => return Err(bad()),
    };
    now.as_second() + n * secs
  } else if let Ok(t) = s.parse::<jiff::Timestamp>() {
    t.as_second()
  } else {
    let civil: jiff::civil::DateTime = s.replace(' ', "T").parse().map_err(|_| {
      Error::input(format!(
        "`{s}` is not a time (+2h, '2026-10-20 09:30' or RFC 3339)"
      ))
    })?;
    civil
      .to_zoned(jiff::tz::TimeZone::system())
      .map_err(|e| Error::input(e.to_string()))?
      .timestamp()
      .as_second()
  };
  if at <= now.as_second() {
    return Err(Error::input(format!("{s} is not in the future")));
  }
  Ok(at)
}

async fn quote(api: &Api, post: &str, text: &str, images: &[PathBuf]) -> Result<Action> {
  social_core::cli::check_images(images)?;
  let draft = Draft {
    text: text.to_owned(),
    images: images.to_vec(),
    quote: Some(post.to_owned()),
    ..Draft::default()
  };
  write::publish(api, &draft).await
}

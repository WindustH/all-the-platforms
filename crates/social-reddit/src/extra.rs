//! Reddit-only commands: browsing one subreddit, a multireddit or a domain,
//! community details (rules, moderators, wiki, flairs), your other lists,
//! downvotes, a user's comments, trophies, post flags and flair, moderator
//! actions, blocking and the inbox's read state.

use clap::builder::PossibleValuesParser;
use social_core::cli::PageArgs;
use social_core::{Data, Page, Result};

use crate::api::Api;
use crate::manage::Flag;
use crate::posts::{SEARCH_SORTS, TIME_RANGES};
use crate::{community, inbox, library, manage, multis, posts, subs, users, write};

const SORTS: [&str; 5] = ["hot", "new", "top", "rising", "controversial"];

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Posts of one subreddit
  #[command(visible_alias = "r")]
  Sub {
    /// Name, r/name or link
    subreddit: String,
    /// Order of the posts
    #[arg(short, long, default_value = "hot",
      value_parser = ["hot", "new", "top", "rising", "controversial"])]
    sort: String,
    /// Time range of `top` and `controversial`
    #[arg(short, long, value_parser = PossibleValuesParser::new(TIME_RANGES.iter().copied()))]
    time: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// About a subreddit: description, subscribers and active users
  Subreddit { subreddit: String },
  /// Popular, new or default subreddits
  Subreddits {
    #[arg(default_value = "popular", value_parser = ["popular", "new", "default"])]
    which: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Downvote a post or comment (`t1_…` id or comment link)
  Downvote {
    post: String,
    /// Clear the vote instead
    #[arg(long)]
    undo: bool,
  },
  /// Comments written by a user
  UserComments {
    user: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Mark inbox items read (all of them when no id is given)
  MarkRead {
    /// `t1_…` / `t4_…` ids from `notifications`
    ids: Vec<String>,
    /// Mark them unread instead
    #[arg(long, requires = "ids")]
    unread: bool,
  },
  /// Search inside one subreddit
  SearchIn {
    subreddit: String,
    query: String,
    #[arg(short, long, default_value = "relevance",
      value_parser = PossibleValuesParser::new(SEARCH_SORTS.iter().copied()))]
    sort: String,
    #[arg(short, long, value_parser = PossibleValuesParser::new(TIME_RANGES.iter().copied()))]
    time: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Posts linking to a domain
  Domain {
    domain: String,
    #[arg(short, long, default_value = "hot", value_parser = SORTS)]
    sort: String,
    #[arg(short, long, value_parser = PossibleValuesParser::new(TIME_RANGES.iter().copied()))]
    time: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// A subreddit's rules
  Rules { subreddit: String },
  /// A subreddit's moderators and their permissions
  #[command(visible_alias = "moderators")]
  Mods { subreddit: String },
  /// A subreddit's wiki: its pages, or one page (`index` is the main one)
  Wiki {
    subreddit: String,
    page: Option<String>,
  },
  /// Post flairs a subreddit offers
  Flairs { subreddit: String },
  /// Set your post's flair: a template from `flairs` and/or a text
  Flair {
    post: String,
    /// Flair text (where the subreddit or template allows editing)
    text: Option<String>,
    /// Template id, or `#N` after `flairs`
    #[arg(long)]
    template: Option<String>,
  },
  /// Your multireddits, or a user's public ones
  #[command(visible_alias = "multireddits")]
  Multis { user: Option<String> },
  /// Posts of a multireddit (`u/USER/m/NAME`, a link or `#N` after `multis`)
  Multi {
    multireddit: String,
    #[arg(short, long, default_value = "hot", value_parser = SORTS)]
    sort: String,
    #[arg(short, long, value_parser = PossibleValuesParser::new(TIME_RANGES.iter().copied()))]
    time: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Comments you saved
  SavedComments {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Posts you hid
  Hidden {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Posts you downvoted
  Downvoted {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Hide a post from your listings
  Hide {
    post: String,
    /// Show it again instead
    #[arg(long)]
    undo: bool,
  },
  /// A user's trophies (yours when USER is omitted)
  Trophies { user: Option<String> },
  /// Subreddits a user moderates (yours when USER is omitted)
  Moderated { user: Option<String> },
  /// Turn a flag of your post or comment on (or off with --undo)
  Flag {
    thing: String,
    #[arg(value_enum)]
    flag: Flag,
    #[arg(long)]
    undo: bool,
  },
  /// Approve a post or comment (moderators)
  Approve { thing: String },
  /// Remove a post or comment (moderators)
  Remove {
    thing: String,
    /// Also mark it as spam
    #[arg(long)]
    spam: bool,
  },
  /// Block a user (or unblock with --undo)
  Block {
    user: String,
    #[arg(long)]
    undo: bool,
  },
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  use Command as C;
  let ctx = &api.ctx;
  Ok(match command {
    C::Sub {
      subreddit,
      sort,
      time,
      page,
    } => Data::Posts(
      page
        .collect_dated(async |r| {
          posts::subreddit(api, &subreddit, &sort, time.as_deref(), &r).await
        })
        .await?,
    ),
    C::Subreddit { subreddit } => {
      Data::Collections(Page::last(vec![subs::about(api, &subreddit).await?]))
    }
    C::Subreddits { which, page } => Data::Collections(
      page
        .collect(async |r| subs::listed(api, &which, &r).await)
        .await?,
    ),
    C::Downvote { post, undo } => {
      let (dir, name) = if undo {
        (0, "unvote")
      } else {
        (-1, "downvote")
      };
      Data::Action(write::vote(api, &ctx.post_ref(&post)?, dir, name).await?)
    }
    C::UserComments { user, page } => {
      let user = ctx.user_ref(&user)?;
      Data::Comments(
        page
          .collect_dated(async |r| users::comments(api, &user, &r).await)
          .await?,
      )
    }
    C::MarkRead { ids, unread } => Data::Action(if ids.is_empty() {
      inbox::mark_read(api).await?
    } else {
      manage::mark(api, &ids, unread).await?
    }),
    C::SearchIn {
      subreddit,
      query,
      sort,
      time,
      page,
    } => Data::Posts(
      page
        .collect_dated(async |r| {
          library::search_in(api, &subreddit, &query, &sort, time.as_deref(), &r).await
        })
        .await?,
    ),
    C::Domain {
      domain,
      sort,
      time,
      page,
    } => Data::Posts(
      page
        .collect_dated(async |r| library::domain(api, &domain, &sort, time.as_deref(), &r).await)
        .await?,
    ),
    C::Rules { subreddit } => Data::Value(community::rules(api, &subreddit).await?),
    C::Mods { subreddit } => Data::Users(community::moderators(api, &subreddit).await?),
    C::Wiki { subreddit, page } => match page {
      None => Data::Collections(community::wiki_pages(api, &subreddit).await?),
      Some(page) => {
        // `#N` after the page list names `SUB/PAGE`; page names have slashes too.
        let resolved = ctx.collection_ref(&page)?;
        let page = match resolved.split_once('/') {
          Some((_, p)) if resolved != page => p.to_owned(),
          _ => resolved,
        };
        Data::Post(Box::new(
          community::wiki_page(api, &subreddit, &page).await?,
        ))
      }
    },
    C::Flairs { subreddit } => Data::Collections(community::link_flairs(api, &subreddit).await?),
    C::Flair {
      post,
      text,
      template,
    } => {
      let template = template.map(|t| ctx.collection_ref(&t)).transpose()?;
      Data::Action(
        manage::flair(
          api,
          &ctx.post_ref(&post)?,
          template.as_deref(),
          text.as_deref(),
        )
        .await?,
      )
    }
    C::Multis { user } => {
      let user = user.map(|u| ctx.user_ref(&u)).transpose()?;
      Data::Collections(multis::list(api, user.as_deref()).await?)
    }
    C::Multi {
      multireddit,
      sort,
      time,
      page,
    } => {
      let multi = ctx.collection_ref(&multireddit)?;
      Data::Posts(
        page
          .collect_dated(async |r| multis::posts(api, &multi, &sort, time.as_deref(), &r).await)
          .await?,
      )
    }
    C::SavedComments { page } => Data::Comments(
      page
        .collect_dated(async |r| library::saved_comments(api, &r).await)
        .await?,
    ),
    C::Hidden { page } => Data::Posts(
      page
        .collect_dated(async |r| posts::own(api, None, "hidden", &r).await)
        .await?,
    ),
    C::Downvoted { page } => Data::Posts(
      page
        .collect_dated(async |r| posts::own(api, None, "downvoted", &r).await)
        .await?,
    ),
    C::Hide { post, undo } => Data::Action(library::hide(api, &ctx.post_ref(&post)?, undo).await?),
    C::Trophies { user } => {
      let user = match user {
        Some(u) => ctx.user_ref(&u)?,
        None => api.username().await?,
      };
      Data::Value(library::trophies(api, &user).await?)
    }
    C::Moderated { user } => {
      let user = user.map(|u| ctx.user_ref(&u)).transpose()?;
      Data::Collections(library::moderated(api, user.as_deref()).await?)
    }
    C::Flag { thing, flag, undo } => {
      Data::Action(manage::flag(api, &ctx.post_ref(&thing)?, flag, undo).await?)
    }
    C::Approve { thing } => Data::Action(manage::approve(api, &ctx.post_ref(&thing)?).await?),
    C::Remove { thing, spam } => {
      Data::Action(manage::remove(api, &ctx.post_ref(&thing)?, spam).await?)
    }
    C::Block { user, undo } => Data::Action(manage::block(api, &ctx.user_ref(&user)?, undo).await?),
  })
}

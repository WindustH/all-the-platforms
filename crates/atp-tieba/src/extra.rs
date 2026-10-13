//! Tieba-only commands: browsing a forum, its details and in-forum search,
//! hot topic threads, a user's replies, following forums and signing in.

use atp_core::cli::PageArgs;
use atp_core::{Data, Error, Page, Post, Query, Result, Value};
use clap::builder::PossibleValuesParser;

use crate::api::Api;
use crate::forums::SORTS;
use crate::search::{SEARCH_FILTERS, SEARCH_SORTS};
use crate::{forums, search, topics, users};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Threads of a forum (吧), by last reply, newest or 精品 only
  #[command(visible_alias = "f", after_long_help = EX_FORUM)]
  Forum {
    /// Forum name or /f?kw= link
    forum: String,
    /// Order of the threads
    #[arg(short, long, default_value = "reply",
      value_parser = PossibleValuesParser::new(SORTS.iter().copied()))]
    sort: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// A forum's details: members, threads, posts, category, managers
  ForumInfo { forum: String },
  /// Search threads inside one forum
  ForumSearch {
    forum: String,
    query: String,
    /// Result order
    #[arg(short, long, default_value = "relevant",
      value_parser = PossibleValuesParser::new(SEARCH_SORTS.iter().copied()))]
    sort: String,
    /// `threads`: thread starts only; `posts`: replies too
    #[arg(long, default_value = "threads",
      value_parser = PossibleValuesParser::new(SEARCH_FILTERS.iter().copied()))]
    filter: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Threads of a hot topic (an item of `hot`)
  Topic {
    /// Topic id, `#N` of `hot`, or a hottopic link
    topic: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Replies a user wrote in threads (yours when USER is omitted)
  UserReplies {
    user: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Follow (关注) a forum
  FollowForum {
    forum: String,
    /// Leave the forum instead
    #[arg(long)]
    undo: bool,
  },
  /// Leave a forum
  UnfollowForum { forum: String },
  /// Sign in (签到) to forums, or to every followed forum with --all
  #[command(after_long_help = EX_SIGN)]
  Sign {
    forums: Vec<String>,
    #[arg(long, conflicts_with = "forums")]
    all: bool,
  },
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    Command::Forum { forum, sort, page } => {
      let forum = ctx.collection_ref(&forum)?;
      Data::Posts(
        page
          .collect_dated(async |r| forums::threads(api, &forum, &sort, &r).await)
          .await?,
      )
    }
    Command::ForumInfo { forum } => {
      let forum = ctx.collection_ref(&forum)?;
      Data::Collections(Page::last(vec![forums::info(api, &forum).await?]))
    }
    Command::ForumSearch {
      forum,
      query,
      sort,
      filter,
      page,
    } => {
      let query = Query {
        keyword: query,
        sort: Some(sort),
        filter: Some(filter),
      };
      Data::Posts(forum_search(api, &forum, &query, page).await?)
    }
    Command::Topic { topic, page } => {
      let topic = ctx.post_ref(&topic)?;
      Data::Posts(
        page
          .collect_dated(async |r| topics::topic(api, &topic, &r).await)
          .await?,
      )
    }
    Command::UserReplies { user, page } => {
      let user = user_or_me(api, user).await?;
      Data::Posts(
        page
          .collect_dated(async |r| users::posts(api, &user, false, &r).await)
          .await?,
      )
    }
    Command::FollowForum { forum, undo } => {
      let forum = ctx.collection_ref(&forum)?;
      Data::Action(forums::follow_forum(api, &forum, undo).await?)
    }
    Command::UnfollowForum { forum } => {
      let forum = ctx.collection_ref(&forum)?;
      Data::Action(forums::follow_forum(api, &forum, true).await?)
    }
    Command::Sign { forums: names, all } => Data::Value(sign(api, &names, all).await?.into()),
  })
}

/// Threads of one forum matching a search.
async fn forum_search(api: &Api, forum: &str, query: &Query, page: PageArgs) -> Result<Page<Post>> {
  let forum = api.ctx.collection_ref(forum)?;
  page
    .collect_dated(async |r| search::search(api, query, Some(&forum), &r).await)
    .await
}

/// A user argument, or the logged-in account when it is omitted.
async fn user_or_me(api: &Api, user: Option<String>) -> Result<String> {
  match user {
    Some(u) => api.ctx.user_ref(&u),
    None => Ok(api.me().await?.uid),
  }
}

/// 签到 in the named forums, or in every followed one with `--all`.
async fn sign(api: &Api, names: &[String], all: bool) -> Result<Vec<Value>> {
  if all {
    return forums::sign_all(api).await;
  }
  if names.is_empty() {
    return Err(Error::input("name the forums to sign in to, or pass --all"));
  }
  let mut rows = Vec::new();
  for name in names {
    rows.push(forums::sign(api, &api.ctx.collection_ref(name)?).await?);
  }
  Ok(rows)
}

const EX_FORUM: &str = "\
Examples:
  atp tieba forum rust -n 50
  atp tieba forum 李毅 --sort good
  atp tieba read '#1'";

const EX_SIGN: &str = "\
Examples:
  atp tieba sign rust 原神      # these forums
  atp tieba sign --all          # every forum you follow";

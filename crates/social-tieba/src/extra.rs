//! Tieba-only commands: browsing a forum, its details and in-forum search,
//! hot topic threads, a user's replies, following forums and signing in.

use clap::builder::PossibleValuesParser;
use social_core::cli::PageArgs;
use social_core::{Data, Page, Query, Result};

use crate::api::Client;
use crate::forum::{SEARCH_FILTERS, SEARCH_SORTS, SORTS};
use crate::{forum, people, write};

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

pub async fn run(c: &Client, command: Command) -> Result<Data> {
  let ctx = &c.ctx;
  Ok(match command {
    Command::Forum { forum, sort, page } => {
      let forum = ctx.collection_ref(&forum)?;
      Data::Posts(
        page
          .collect_dated(async |r| forum::threads(c, &forum, &sort, &r).await)
          .await?,
      )
    }
    Command::ForumInfo { forum } => {
      let forum = ctx.collection_ref(&forum)?;
      Data::Collections(Page::last(vec![forum::info(c, &forum).await?]))
    }
    Command::ForumSearch {
      forum,
      query,
      sort,
      filter,
      page,
    } => {
      let forum = ctx.collection_ref(&forum)?;
      let q = Query {
        keyword: query,
        sort: Some(sort),
        filter: Some(filter),
      };
      Data::Posts(
        page
          .collect_dated(async |r| forum::search(c, &q, Some(&forum), &r).await)
          .await?,
      )
    }
    Command::Topic { topic, page } => {
      let topic = ctx.post_ref(&topic)?;
      Data::Posts(
        page
          .collect_dated(async |r| forum::topic(c, &topic, &r).await)
          .await?,
      )
    }
    Command::UserReplies { user, page } => {
      let user = match user {
        Some(u) => ctx.user_ref(&u)?,
        None => c.me().await?.uid,
      };
      Data::Posts(
        page
          .collect_dated(async |r| people::posts(c, &user, false, &r).await)
          .await?,
      )
    }
    Command::FollowForum { forum, undo } => {
      let forum = ctx.collection_ref(&forum)?;
      Data::Action(write::follow_forum(c, &forum, undo).await?)
    }
    Command::UnfollowForum { forum } => {
      let forum = ctx.collection_ref(&forum)?;
      Data::Action(write::follow_forum(c, &forum, true).await?)
    }
    Command::Sign { forums, all } => {
      let rows = if all {
        write::sign_all(c).await?
      } else if forums.is_empty() {
        return Err(social_core::Error::input(
          "name the forums to sign in to, or pass --all",
        ));
      } else {
        let mut rows = Vec::new();
        for f in forums {
          rows.push(write::sign(c, &ctx.collection_ref(&f)?).await?);
        }
        rows
      };
      Data::Value(rows.into())
    }
  })
}

const EX_FORUM: &str = "\
Examples:
  social tieba forum rust -n 50
  social tieba forum 李毅 --sort good
  social tieba read '#1'";

const EX_SIGN: &str = "\
Examples:
  social tieba sign rust 原神      # these forums
  social tieba sign --all          # every forum you follow";

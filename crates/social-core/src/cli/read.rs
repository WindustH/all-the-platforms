//! Reading commands: search, hot, feed, posts, comments, users and analytics.

use clap::Subcommand;

use super::args::PageArgs;
use crate::error::Result;
use crate::model::{Comment, Data};
use crate::paging::collect;
use crate::platform::{Cap, Platform, Query};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum SearchKind {
  Post,
  User,
  Topic,
}

#[derive(Debug, clap::Args)]
pub struct SearchArgs {
  pub query: String,
  /// What to search for
  #[arg(short = 't', long = "type", value_enum, default_value_t = SearchKind::Post)]
  pub kind: SearchKind,
  /// Result order
  #[arg(short, long)]
  pub sort: Option<String>,
  /// Content filter
  #[arg(long)]
  pub filter: Option<String>,
  #[command(flatten)]
  pub page: PageArgs,
}

#[derive(Debug, Subcommand)]
pub enum ReadCommand {
  /// Search posts, users or topics
  #[command(after_long_help = EX_SEARCH)]
  Search(SearchArgs),
  /// Trending content
  Hot {
    #[arg(short, long)]
    category: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Your timeline / recommendations
  Feed {
    #[arg(short = 't', long = "type")]
    kind: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Show one post
  Read { post: String },
  /// Comments of a post
  #[command(after_long_help = EX_COMMENTS)]
  Comments {
    post: String,
    #[arg(short, long)]
    sort: Option<String>,
    /// Fetch every comment (ignores --limit)
    #[arg(long)]
    all: bool,
    /// Also fetch every reply under each comment
    #[arg(long)]
    replies: bool,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Accounts that liked a post
  Likers {
    post: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Reposts, retweets, quotes or crossposts of a post
  Reposts {
    post: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Creator analytics of your account, or of one of your posts
  #[command(after_long_help = EX_INSIGHTS)]
  Insights {
    post: Option<String>,
    /// Days of history for trends
    #[arg(short, long, default_value_t = 30)]
    days: u32,
  },
  /// Replies under one comment
  Replies {
    post: String,
    comment: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Show a user profile
  User { user: String },
  /// Posts of a user (yours when USER is omitted)
  #[command(after_long_help = EX_USER_POSTS)]
  UserPosts {
    user: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Followers of a user (yours when USER is omitted)
  Followers {
    user: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Accounts a user follows (yours when USER is omitted)
  Following {
    user: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
}

pub(super) async fn run<P: Platform>(p: &P, cmd: ReadCommand) -> Result<Data> {
  let ctx = p.ctx();
  match cmd {
    ReadCommand::Search(args) => search(p, args).await,
    ReadCommand::Hot { category, page } => hot(p, category.as_deref(), page).await,
    ReadCommand::Feed { kind, page } => feed(p, kind.as_deref(), page).await,
    ReadCommand::Read { post } => Ok(Data::Post(Box::new(p.read(&ctx.post_ref(&post)?).await?))),
    ReadCommand::Comments {
      post,
      sort,
      all,
      replies,
      page,
    } => {
      comments(
        p,
        &ctx.post_ref(&post)?,
        sort.as_deref(),
        all,
        replies,
        page,
      )
      .await
    }
    ReadCommand::Replies {
      post,
      comment,
      page,
    } => comment_replies(p, &ctx.post_ref(&post)?, &comment, page).await,
    ReadCommand::Likers { post, page } => likers(p, &ctx.post_ref(&post)?, page).await,
    ReadCommand::Reposts { post, page } => reposts(p, &ctx.post_ref(&post)?, page).await,
    ReadCommand::Insights { post, days } => insights(p, post, days).await,
    ReadCommand::User { user } => Ok(Data::User(Box::new(p.user(&ctx.user_ref(&user)?).await?))),
    ReadCommand::UserPosts { user, page } => user_posts(p, &user_or_me(p, user).await?, page).await,
    ReadCommand::Followers { user, page } => followers(p, &user_or_me(p, user).await?, page).await,
    ReadCommand::Following { user, page } => following(p, &user_or_me(p, user).await?, page).await,
  }
}

async fn hot<P: Platform>(p: &P, category: Option<&str>, page: PageArgs) -> Result<Data> {
  Ok(Data::Posts(
    page
      .collect_dated(async |r| p.hot(category, &r).await)
      .await?,
  ))
}

async fn feed<P: Platform>(p: &P, kind: Option<&str>, page: PageArgs) -> Result<Data> {
  Ok(Data::Posts(
    page.collect_dated(async |r| p.feed(kind, &r).await).await?,
  ))
}

async fn comment_replies<P: Platform>(
  p: &P,
  post: &str,
  comment: &str,
  page: PageArgs,
) -> Result<Data> {
  let replies = page
    .collect_dated(async |r| p.replies(post, comment, &r).await)
    .await?;
  Ok(Data::Comments(replies))
}

async fn likers<P: Platform>(p: &P, post: &str, page: PageArgs) -> Result<Data> {
  Ok(Data::Users(
    page.collect(async |r| p.likers(post, &r).await).await?,
  ))
}

async fn reposts<P: Platform>(p: &P, post: &str, page: PageArgs) -> Result<Data> {
  Ok(Data::Posts(
    page
      .collect_dated(async |r| p.reposts(post, &r).await)
      .await?,
  ))
}

async fn insights<P: Platform>(p: &P, post: Option<String>, days: u32) -> Result<Data> {
  let post = post.map(|x| p.ctx().post_ref(&x)).transpose()?;
  let insights = p.insights(post.as_deref(), days.max(1)).await?;
  Ok(Data::Insights(Box::new(insights)))
}

async fn user_posts<P: Platform>(p: &P, user: &str, page: PageArgs) -> Result<Data> {
  Ok(Data::Posts(
    page
      .collect_dated(async |r| p.user_posts(user, &r).await)
      .await?,
  ))
}

async fn followers<P: Platform>(p: &P, user: &str, page: PageArgs) -> Result<Data> {
  Ok(Data::Users(
    page.collect(async |r| p.followers(user, &r).await).await?,
  ))
}

async fn following<P: Platform>(p: &P, user: &str, page: PageArgs) -> Result<Data> {
  Ok(Data::Users(
    page.collect(async |r| p.following(user, &r).await).await?,
  ))
}

async fn search<P: Platform>(p: &P, args: SearchArgs) -> Result<Data> {
  let query = Query {
    keyword: args.query,
    sort: args.sort,
    filter: args.filter,
  };
  let (q, page) = (&query, args.page);
  Ok(match args.kind {
    SearchKind::Post => Data::Posts(page.collect_dated(async |r| p.search(q, &r).await).await?),
    SearchKind::User => Data::Users(page.collect(async |r| p.search_users(q, &r).await).await?),
    SearchKind::Topic => {
      Data::Collections(page.collect(async |r| p.search_topics(q, &r).await).await?)
    }
  })
}

async fn comments<P: Platform>(
  p: &P,
  post: &str,
  sort: Option<&str>,
  all: bool,
  replies: bool,
  mut page: PageArgs,
) -> Result<Data> {
  if all {
    page.limit = usize::MAX;
  }
  let mut comments = page
    .collect_dated(async |r| p.comments(post, sort, &r).await)
    .await?;
  if replies {
    complete_replies(p, post, &mut comments.items, 0).await?;
  }
  Ok(Data::Comments(comments))
}

/// How deep `comments --replies` follows nested threads.
const REPLY_DEPTH: usize = 3;

/// Fetch the full reply thread of every comment whose inline replies are
/// incomplete (platforms usually inline only a few), down to [`REPLY_DEPTH`].
async fn complete_replies<P: Platform>(
  p: &P,
  post: &str,
  comments: &mut [Comment],
  depth: usize,
) -> Result<()> {
  if !P::INFO.supports(Cap::Replies) || depth >= REPLY_DEPTH {
    return Ok(());
  }
  for c in comments.iter_mut() {
    let expected = c.reply_count.unwrap_or(0) as usize;
    if expected > 0 && c.replies.len() < expected {
      let id = c.id.clone();
      let all = collect(usize::MAX, None, async |r| p.replies(post, &id, &r).await).await?;
      c.replies = all.items;
    }
    Box::pin(complete_replies(p, post, &mut c.replies, depth + 1)).await?;
  }
  Ok(())
}

/// A user argument, or the logged-in account when it is omitted.
async fn user_or_me<P: Platform>(p: &P, user: Option<String>) -> Result<String> {
  match user {
    Some(u) => p.ctx().user_ref(&u),
    None => Ok(p.whoami().await?.reference().to_owned()),
  }
}

const EX_SEARCH: &str = "\
Examples:
  social bili search \"rust 教程\" -n 10
  social yt search \"rust\" --sort popularity --filter week
  social xhs search 咖啡 --sort popular --filter video
  social x search \"AI agent\" --sort latest --since 1d -f csv > tweets.csv
  social bili search 影视飓风 -t user
  social zhihu search rust -t topic

Values of --sort / --filter differ per platform: see `social guide <platform>`.";

const EX_COMMENTS: &str = "\
Examples:
  social bili comments BV1xx411c7mD -n 50
  social bili comments '#1' --sort time
  social reddit comments 1wsxldl --all --replies -f jsonl > thread.jsonl
  social yt comments dQw4w9WgXcQ -n 500 --since 30d -f csv > comments.csv

--all fetches every comment; --replies also every reply under each (CSV and
JSON Lines rows carry depth and parent_id). More: social guide analysis";

const EX_INSIGHTS: &str = "\
Examples:
  social bili insights                   # your account, last 30 days
  social bili insights --days 7 -f csv > week.csv
  social bili insights BV1xx411c7mD      # one of your videos
  social x insights 2104695667180380260  # one of your tweets

Totals, daily series and breakdowns (traffic sources, audience ...); warnings
say what a platform withholds. Others' posts get public counters.
More: social guide analysis, social guide <platform>";

const EX_USER_POSTS: &str = "\
Examples:
  social bili user-posts 946974 -n 100 --since 90d -f csv > videos.csv
  social x user-posts NASA -n 50
  social yt user-posts @NASA -n 20
  social bili user-posts                 # your own videos";

//! Lemmy commands beyond the shared set.

use atp_core::cli::{PageArgs, read_text};
use atp_core::{Data, Error, Page, Result};

use crate::api::Api;
use crate::posts::{self, Submission};
use crate::{communities, users};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Posts of one community
  #[command(visible_alias = "c", after_long_help = EX_COMMUNITY)]
  Community {
    /// Community: !name@host, !name, c/name, a link, or #N
    community: String,
    /// Order: hot, active, scaled, new, old, controversial, most-comments,
    /// new-comments, top-hour, top-day, top-week, top-month, top-year, top-all
    #[arg(short, long)]
    sort: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// A community: title, description, subscribers, posts and activity
  CommunityInfo {
    /// Community: !name@host, !name, c/name, a link, or #N
    community: String,
  },
  /// Communities the instance knows, its own, or the ones you subscribe to
  Communities {
    /// all, local or subscribed (needs an account)
    #[arg(short = 't', long = "type", default_value = "all")]
    listing: String,
    /// active, hot, new, old, top-day, top-week, top-month, top-year, top-all
    #[arg(short, long, default_value = "active")]
    sort: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Subscribe to a community (needs an account)
  Subscribe {
    /// Community: !name@host, !name, c/name, a link, or #N
    community: String,
    /// Unsubscribe instead
    #[arg(long)]
    undo: bool,
  },
  /// Vote on a post or comment (needs an account)
  #[command(after_long_help = EX_VOTE)]
  Vote {
    /// Post or comment: a link, post/<id>, comment/<id>, or #N
    target: String,
    /// up, down, or clear (take your vote back)
    direction: String,
  },
  /// Submit a post with a title, a link and/or text (needs an account)
  #[command(after_long_help = EX_SUBMIT)]
  Submit {
    /// Community: !name@host, !name, c/name, or a link
    community: String,
    /// Title of the post
    #[arg(long)]
    title: String,
    /// A link the post points to
    #[arg(long)]
    url: Option<String>,
    /// Text of the post (Markdown); `-` reads it from stdin
    #[arg(long)]
    body: Option<String>,
    /// Mark it NSFW
    #[arg(long)]
    nsfw: bool,
  },
  /// The instance: name, version, sign-ups and counts
  Instance,
}

const EX_COMMUNITY: &str = "\
Examples:
  atp lemmy community '!rust@programming.dev' --sort top-week -n 10
  atp lemmy c linux --sort new

More: atp guide lemmy community";

const EX_VOTE: &str = "\
Examples:
  atp lemmy vote '#1' up
  atp lemmy vote https://lemmy.world/comment/123456 down
  atp lemmy vote post/987654 clear

More: atp guide lemmy vote";

const EX_SUBMIT: &str = "\
Examples:
  atp lemmy submit '!rust@programming.dev' --title 'A crate I made' --url https://example.com
  atp lemmy submit '!asklemmy@lemmy.world' --title 'A question' --body -

More: atp guide lemmy submit";

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    Command::Community {
      community,
      sort,
      page,
    } => community_posts(api, &community, sort.as_deref(), page).await?,
    Command::CommunityInfo { community } => {
      let community = communities::info(api, &ctx.collection_ref(&community)?).await?;
      Data::Collections(Page::last(vec![community]))
    }
    Command::Communities {
      listing,
      sort,
      page,
    } => Data::Collections(
      page
        .collect(async |req| communities::list(api, &listing, &sort, &req).await)
        .await?,
    ),
    Command::Subscribe { community, undo } => {
      let community = ctx.collection_ref(&community)?;
      Data::Action(communities::subscribe(api, &community, undo).await?)
    }
    Command::Vote { target, direction } => {
      Data::Action(posts::vote(api, &ctx.post_ref(&target)?, score(&direction)?).await?)
    }
    Command::Submit {
      community,
      title,
      url,
      body,
      nsfw,
    } => {
      let post = Submission {
        community: ctx.collection_ref(&community)?,
        title,
        link: url,
        body: body.map(|b| read_text(Some(b))).transpose()?,
        nsfw,
      };
      Data::Action(posts::submit(api, post).await?)
    }
    Command::Instance => Data::Value(users::instance(api).await?),
  })
}

async fn community_posts(api: &Api, arg: &str, sort: Option<&str>, page: PageArgs) -> Result<Data> {
  let community = api.ctx.collection_ref(arg)?;
  Ok(Data::Posts(
    page
      .collect_dated(async |req| posts::community(api, &community, sort, &req).await)
      .await?,
  ))
}

/// `up`, `down`, `clear` as Lemmy's vote scores.
fn score(direction: &str) -> Result<i8> {
  match direction {
    "up" => Ok(1),
    "down" => Ok(-1),
    "clear" => Ok(0),
    other => Err(Error::input(format!("vote up, down or clear, not {other}"))),
  }
}

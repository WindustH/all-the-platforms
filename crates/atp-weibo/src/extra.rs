//! Weibo-only commands: reposts with text, posts under a hashtag, the
//! posts of a follow group, your following filtered, special follows and
//! remarks.

use atp_core::cli::{PageArgs, read_text};
use atp_core::{Data, Page, PageReq, Post, Result};

use crate::api::Api;
use crate::follows::{self, Filter};
use crate::{feeds, posts, relations, search};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Repost (转发) a post, optionally with your own text
  #[command(after_long_help = EX_REPOST)]
  Repost {
    /// Post: id, short id (`bid`), link or #N
    post: String,
    /// Your text; `-` reads it from stdin
    text: Option<String>,
    /// Also post the text as a comment on the original
    #[arg(long)]
    comment: bool,
  },
  /// Posts under a hashtag (话题), newest first
  Topic {
    /// Topic name, with or without the surrounding `#`
    name: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Latest posts of one of your follow groups
  #[command(after_long_help = EX_GROUP)]
  Group {
    /// Group id from `follow-groups`, or #N after it
    gid: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Your following, by when you followed them or gone quiet
  #[command(after_long_help = EX_FOLLOWING_FILTER)]
  FollowingFilter {
    /// Followed within the last 7 days, 30 days or half a year
    #[arg(long, value_enum, required_unless_present = "inactive")]
    within: Option<Filter>,
    /// Posted nothing in the last 30 days
    #[arg(long, conflicts_with = "within")]
    inactive: bool,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Your special follows (特别关注)
  SpecialFollows {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Mark users as special follows (特别关注), or unmark them with --undo
  #[command(after_long_help = EX_SPECIAL)]
  Special {
    /// Users: ids, `@names`, links, or #N from the last user list
    #[arg(required = true, value_name = "USER")]
    users: Vec<String>,
    /// Unmark them
    #[arg(long)]
    undo: bool,
  },
  /// Set the remark (备注) you see instead of a user's name
  Remark {
    /// User: id, `@name`, link or #N
    user: String,
    /// The remark; empty clears it
    text: String,
  },
}

const EX_REPOST: &str = "\
Examples:
  atp weibo repost '#3'
  atp weibo repost https://weibo.com/2761630725/RmbnfkgUR '说得好' --comment

More: atp guide weibo repost";

const EX_GROUP: &str = "\
Examples:
  atp weibo follow-groups
  atp weibo group '#2'

More: atp guide weibo group";

const EX_FOLLOWING_FILTER: &str = "\
Examples:
  atp weibo following-filter --within 7d
  atp weibo following-filter --inactive -n 200
  atp weibo unfollow '#3' '#5' '#8'

More: atp guide weibo following-filter";

const EX_SPECIAL: &str = "\
Examples:
  atp weibo special 1749127163 '@央视新闻'
  atp weibo special-follows
  atp weibo special '#2' --undo

More: atp guide weibo special";

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    Command::Repost {
      post,
      text,
      comment,
    } => {
      let text = read_text(text)?;
      Data::Action(posts::repost(api, &ctx.post_ref(&post)?, &text, comment).await?)
    }
    Command::Topic { name, page } => Data::Posts(
      page
        .collect_dated(async |r| search::topic(api, &name, &r).await)
        .await?,
    ),
    Command::Group { gid, page } => {
      let gid = ctx.collection_ref(&gid)?;
      Data::Posts(
        page
          .collect_dated(async |r| group(api, &gid, &r).await)
          .await?,
      )
    }
    Command::FollowingFilter {
      within,
      inactive,
      page,
    } => Data::Users(
      page
        .collect(async |r| follows::filtered(api, within, inactive, &r).await)
        .await?,
    ),
    Command::SpecialFollows { page } => Data::Users(
      page
        .collect(async |r| follows::special(api, &r).await)
        .await?,
    ),
    Command::Special { users, undo } => {
      let users = users
        .iter()
        .map(|u| ctx.user_ref(u))
        .collect::<Result<Vec<_>>>()?;
      Data::actions(relations::special(api, &users, undo).await?)
    }
    Command::Remark { user, text } => {
      Data::Action(relations::remark(api, &ctx.user_ref(&user)?, text.trim()).await?)
    }
  })
}

/// The latest posts of a follow group (`groupstimeline`).
async fn group(api: &Api, gid: &str, page: &PageReq) -> Result<Page<Post>> {
  api.require_login()?;
  let max_id = page.cursor.clone().unwrap_or_default();
  let count = page.size_within(20).to_string();
  let v = api
    .get(
      "/ajax/feed/groupstimeline",
      &[
        ("list_id", gid),
        ("refresh", "4"),
        ("fast_refresh", "1"),
        ("max_id", &max_id),
        ("count", &count),
      ],
    )
    .await?;
  Ok(feeds::timeline(&v))
}

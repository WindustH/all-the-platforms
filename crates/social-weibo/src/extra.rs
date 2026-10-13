//! Weibo-only commands: reposts with text, posts under a hashtag, favorite
//! folders and feed groups.

use social_core::cli::{PageArgs, read_text};
use social_core::{Collection, Data, Page, PageReq, Post, Result, ValueExt};

use crate::api::Api;
use crate::{feeds, posts, search, users};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Repost (转发) a post, optionally with your own text
  #[command(after_long_help = EX_REPOST)]
  Repost {
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
  /// Your favorite folders (ids for `favorites --folder`)
  Folders,
  /// Your feed groups (ids for `group`)
  Groups,
  /// Latest posts of one of your feed groups
  Group {
    /// Group id from `groups`
    gid: String,
    #[command(flatten)]
    page: PageArgs,
  },
}

const EX_REPOST: &str = "\
Examples:
  social weibo repost '#3'
  social weibo repost https://weibo.com/2761630725/RmbnfkgUR '说得好' --comment";

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
    Command::Folders => Data::Collections(users::folders(api).await?),
    Command::Groups => Data::Collections(groups(api).await?),
    Command::Group { gid, page } => {
      let gid = ctx.collection_ref(&gid)?;
      Data::Posts(
        page
          .collect_dated(async |r| group(api, &gid, &r).await)
          .await?,
      )
    }
  })
}

async fn groups(api: &Api) -> Result<Page<Collection>> {
  api.require_login()?;
  let v = api
    .get(
      "/ajax/feed/allGroups",
      &[("is_new_segment", "1"), ("fetch_hot", "1")],
    )
    .await?;
  let mut items = Vec::new();
  for section in v.list("groups") {
    for g in section.list("group") {
      let Some(id) = g.first_str(&["gid", "idstr"]) else {
        continue;
      };
      let mut col = Collection {
        id,
        kind: "group".into(),
        name: g.str("title").unwrap_or_default(),
        description: section.str("title"),
        raw: Some(g.clone()),
        ..Collection::default()
      };
      if let Some(n) = g.count("member_count") {
        col.items = Some(n);
      }
      items.push(col);
    }
  }
  Ok(Page::last(items))
}

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

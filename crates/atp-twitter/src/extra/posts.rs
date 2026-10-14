//! Retweets and quotes, who retweeted, profile tabs and similar posts.

use std::path::PathBuf;

use atp_core::cli::PageArgs;
use atp_core::{Action, Data, Draft, Result};

use crate::api::Api;
use crate::tabs::{self, Tab};
use crate::{users, write};

const EX_TAB: &str = "\
Examples:
  atp x tab NASA media -n 50
  atp x tab @NASA replies --since 7d";

#[derive(Debug, clap::Subcommand)]
pub enum PostCommand {
  /// Retweet a tweet
  #[command(visible_alias = "repost")]
  Retweet {
    /// Tweet: id, link or #N
    post: String,
    /// Undo the retweet instead
    #[arg(long)]
    undo: bool,
  },
  /// Quote a tweet with your own text
  Quote {
    /// Tweet: id, link or #N
    post: String,
    /// Your text
    text: String,
    /// Attach an image (repeatable, at most 4)
    #[arg(short = 'i', long = "image", value_name = "PATH")]
    images: Vec<PathBuf>,
  },
  /// Accounts that retweeted a tweet (its quotes: `reposts`)
  Retweeters {
    /// Tweet: id, link or #N
    post: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// A profile tab: replies, media, highlights or articles
  #[command(after_long_help = EX_TAB)]
  Tab {
    /// User: handle, @handle, link, id or #N
    user: String,
    /// Which tab
    #[arg(value_enum)]
    tab: Tab,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Verified followers of a user
  VerifiedFollowers {
    /// User: handle, @handle, link, id or #N
    user: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Followers of a user that you follow too
  FollowersYouKnow {
    /// User: handle, @handle, link, id or #N
    user: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Posts similar to a tweet (X Premium)
  Similar {
    /// Tweet: id, link or #N
    post: String,
  },
  /// Earlier versions of an edited tweet
  Edits {
    /// Tweet: id, link or #N
    post: String,
  },
}

pub(super) async fn run(api: &Api, command: PostCommand) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    PostCommand::Retweet { post, undo } => {
      Data::Action(write::retweet(api, &ctx.post_ref(&post)?, undo).await?)
    }
    PostCommand::Quote { post, text, images } => {
      Data::Action(quote(api, &ctx.post_ref(&post)?, &text, &images).await?)
    }
    PostCommand::Retweeters { post, page } => {
      let post = ctx.post_ref(&post)?;
      Data::Users(
        page
          .collect(async |r| users::retweeters(api, &post, &r).await)
          .await?,
      )
    }
    PostCommand::Tab { user, tab, page } => {
      let user = ctx.user_ref(&user)?;
      Data::Posts(
        page
          .collect_dated(async |r| tabs::tab(api, &user, tab, &r).await)
          .await?,
      )
    }
    PostCommand::VerifiedFollowers { user, page } => {
      let user = ctx.user_ref(&user)?;
      Data::Users(
        page
          .collect(async |r| tabs::followers(api, &user, true, &r).await)
          .await?,
      )
    }
    PostCommand::FollowersYouKnow { user, page } => {
      let user = ctx.user_ref(&user)?;
      Data::Users(
        page
          .collect(async |r| tabs::followers(api, &user, false, &r).await)
          .await?,
      )
    }
    PostCommand::Similar { post } => Data::Posts(tabs::similar(api, &ctx.post_ref(&post)?).await?),
    PostCommand::Edits { post } => Data::Posts(tabs::edits(api, &ctx.post_ref(&post)?).await?),
  })
}

async fn quote(api: &Api, post: &str, text: &str, images: &[PathBuf]) -> Result<Action> {
  atp_core::cli::check_images(images)?;
  let draft = Draft {
    text: text.to_owned(),
    images: images.to_vec(),
    quote: Some(post.to_owned()),
    ..Draft::default()
  };
  write::publish(api, &draft).await
}

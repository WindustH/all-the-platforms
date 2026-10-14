//! Your other lists: saved comments, hidden and downvoted posts, trophies,
//! moderated subreddits.

use atp_core::cli::PageArgs;
use atp_core::{Data, Result};

use crate::api::Api;
use crate::{library, posts};

#[derive(Debug, clap::Subcommand)]
pub enum LibraryCommand {
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
    /// Post: id, `t3_…`, link or #N
    post: String,
    /// Show it again instead
    #[arg(long)]
    undo: bool,
  },
  /// A user's trophies (yours when USER is omitted)
  Trophies {
    /// User: name, `u/name` or profile link
    user: Option<String>,
  },
  /// Subreddits a user moderates (yours when USER is omitted)
  Moderated {
    /// User: name, `u/name` or profile link
    user: Option<String>,
  },
}

pub(super) async fn run(api: &Api, command: LibraryCommand) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    LibraryCommand::SavedComments { page } => Data::Comments(
      page
        .collect_dated(async |r| library::saved_comments(api, &r).await)
        .await?,
    ),
    LibraryCommand::Hidden { page } => Data::Posts(
      page
        .collect_dated(async |r| posts::own(api, None, "hidden", &r).await)
        .await?,
    ),
    LibraryCommand::Downvoted { page } => Data::Posts(
      page
        .collect_dated(async |r| posts::own(api, None, "downvoted", &r).await)
        .await?,
    ),
    LibraryCommand::Hide { post, undo } => {
      Data::Action(library::hide(api, &ctx.post_ref(&post)?, undo).await?)
    }
    LibraryCommand::Trophies { user } => {
      let user = match user {
        Some(u) => ctx.user_ref(&u)?,
        None => api.username().await?,
      };
      Data::Value(library::trophies(api, &user).await?)
    }
    LibraryCommand::Moderated { user } => {
      let user = user.map(|u| ctx.user_ref(&u)).transpose()?;
      Data::Collections(library::moderated(api, user.as_deref()).await?)
    }
  })
}

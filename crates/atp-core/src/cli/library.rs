//! Your library: folders, saved and liked posts, history, notifications.

use clap::Subcommand;

use super::args::PageArgs;
use crate::error::Result;
use crate::model::Data;
use crate::platform::Platform;

#[derive(Debug, Subcommand)]
pub enum LibraryCommand {
  /// Favorites folders / lists (yours when USER is omitted)
  Collections {
    /// User: an id, a handle or a link, or #N; yours when omitted
    user: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Saved / bookmarked posts (yours when USER is omitted)
  Favorites {
    /// User: an id, a handle or a link, or #N; yours when omitted
    user: Option<String>,
    /// Only this folder / collection id
    #[arg(long)]
    folder: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Liked posts (yours when USER is omitted)
  Likes {
    /// User: an id, a handle or a link, or #N; yours when omitted
    user: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Your viewing history
  History {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Your notifications
  Notifications {
    /// Only this kind (values differ per platform)
    #[arg(short = 't', long = "type")]
    kind: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Unread counters
  Unread,
}

pub(super) async fn run<P: Platform>(p: &P, cmd: LibraryCommand) -> Result<Data> {
  let ctx = p.ctx();
  let user_ref = |u: Option<String>| u.map(|u| ctx.user_ref(&u)).transpose();
  Ok(match cmd {
    LibraryCommand::Collections { user, page } => {
      let user = user_ref(user)?;
      let user = user.as_deref();
      Data::Collections(
        page
          .collect(async |r| p.collections(user, &r).await)
          .await?,
      )
    }
    LibraryCommand::Favorites { user, folder, page } => {
      let user = user_ref(user)?;
      let folder = folder.map(|f| ctx.collection_ref(&f)).transpose()?;
      let (user, folder) = (user.as_deref(), folder.as_deref());
      Data::Posts(
        page
          .collect_dated(async |r| p.favorites(user, folder, &r).await)
          .await?,
      )
    }
    LibraryCommand::Likes { user, page } => {
      let user = user_ref(user)?;
      let user = user.as_deref();
      Data::Posts(
        page
          .collect_dated(async |r| p.likes(user, &r).await)
          .await?,
      )
    }
    LibraryCommand::History { page } => {
      Data::Posts(page.collect_dated(async |r| p.history(&r).await).await?)
    }
    LibraryCommand::Notifications { kind, page } => {
      let kind = kind.as_deref();
      Data::Notifications(
        page
          .collect_dated(async |r| p.notifications(kind, &r).await)
          .await?,
      )
    }
    LibraryCommand::Unread => Data::Counts(p.unread().await?),
  })
}

//! Bilibili-only commands, grouped by domain in the submodules.

mod explore;
mod interact;
mod library;

use atp_core::cli::PageArgs;
use atp_core::{Data, Error, Result};

use crate::api::Api;
use crate::refs::{self, Video};
use crate::{discover, dynamics, insights, media, users, videos};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Give coins to a video
  Coin {
    video: String,
    /// Number of coins
    #[arg(short = 'n', long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=2))]
    count: u8,
    /// Also like the video
    #[arg(long)]
    like: bool,
  },
  /// Like, coin and favorite a video at once
  Triple { video: String },
  /// Subtitles of a video (most tracks need a login)
  Subtitle {
    video: String,
    /// Track language, e.g. zh-CN, ai-zh, en
    #[arg(long)]
    lang: Option<String>,
  },
  /// AI summary and outline of a video
  Summary { video: String },
  /// Danmaku (bullet comments) of a video
  Danmaku { video: String },
  /// Videos related to a video
  Related {
    video: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Ranking board: the whole site, or one region (see `hot --category`)
  Rank {
    #[arg(long, value_parser = clap::builder::PossibleValuesParser::new(videos::HOT_CATEGORIES))]
    region: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Your watch-later list
  WatchLater,
  /// Your videos side by side with creator-center numbers (3-second bounce,
  /// share watched, follows, click-through rank ...)
  #[command(visible_alias = "my-videos")]
  VideoStats {
    /// Only these videos (at most 10); default: the latest ones
    videos: Vec<String>,
    /// How many of the latest videos
    #[arg(short = 'n', long, default_value_t = 10, value_parser = clap::value_parser!(u16).range(1..=50))]
    limit: u16,
  },
  #[command(flatten)]
  Explore(explore::ExploreCommand),
  #[command(flatten)]
  Library(library::LibraryCommand),
  #[command(flatten)]
  Interact(interact::InteractCommand),
  /// Dynamics of a user (yours when USER is omitted)
  #[command(visible_alias = "my-dynamics")]
  Dynamics {
    user: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
}

async fn video_ref(api: &Api, arg: &str) -> Result<Video> {
  refs::video(api, &api.ctx.post_ref(arg)?).await
}

/// Destructive commands need `--yes`.
fn confirm(yes: bool, what: &str) -> Result<()> {
  if yes {
    Ok(())
  } else {
    Err(Error::input(format!(
      "this would {what}; add --yes to go ahead"
    )))
  }
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  use Command as E;
  Ok(match command {
    E::Coin { video, count, like } => {
      let v = video_ref(api, &video).await?;
      Data::Action(videos::coin(api, &v, count, like).await?)
    }
    E::Triple { video } => Data::Action(videos::triple(api, &video_ref(api, &video).await?).await?),
    E::Subtitle { video, lang } => {
      let v = video_ref(api, &video).await?;
      Data::Transcript(media::subtitle(api, &v, lang.as_deref()).await?)
    }
    E::Summary { video } => Data::Value(media::summary(api, &video_ref(api, &video).await?).await?),
    E::Danmaku { video } => {
      Data::Transcript(media::danmaku(api, &video_ref(api, &video).await?).await?)
    }
    E::Related { video, page } => {
      let v = video_ref(api, &video).await?;
      Data::Posts(
        page
          .collect_dated(async |_| videos::related(api, &v).await)
          .await?,
      )
    }
    E::Rank { region, page } => Data::Posts(
      page
        .collect_dated(async |_| discover::rank(api, region.as_deref()).await)
        .await?,
    ),
    E::Explore(c) => explore::run(api, c).await?,
    E::Library(c) => library::run(api, c).await?,
    E::Interact(c) => interact::run(api, c).await?,
    E::WatchLater => Data::Posts(videos::watch_later(api).await?),
    E::VideoStats { videos, limit } => {
      let mut refs = Vec::new();
      for v in &videos {
        refs.push(video_ref(api, v).await?);
      }
      Data::Posts(insights::compare(api, &refs, limit.into()).await?)
    }
    E::Dynamics { user, page } => {
      let mid = match user {
        Some(u) => users::mid(api, &api.ctx.user_ref(&u)?).await?,
        None => users::my_mid(api).await?,
      };
      Data::Posts(
        page
          .collect_dated(async |r| dynamics::of_user(api, &mid, &r).await)
          .await?,
      )
    }
  })
}

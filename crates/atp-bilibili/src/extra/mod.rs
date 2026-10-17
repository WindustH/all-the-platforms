//! Bilibili-only commands, grouped by domain in the submodules.

mod explore;
mod interact;
mod library;

use atp_core::cli::PageArgs;
use atp_core::{Data, Result};

use crate::api::Api;
use crate::refs::{self, Video};
use crate::{discover, dynamics, insights, media, users, videos};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Give coins to a video
  #[command(after_long_help = EX_COIN)]
  Coin {
    /// Video: BV… or av… id, link, b23.tv short link or #N
    video: String,
    /// Number of coins
    #[arg(short = 'n', long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=2))]
    count: u8,
    /// Also like the video
    #[arg(long)]
    like: bool,
  },
  /// Like, coin and favorite a video at once
  Triple {
    /// Video: BV… or av… id, link, b23.tv short link or #N
    video: String,
  },
  /// Subtitles of a video (most tracks need a login)
  #[command(after_long_help = EX_SUBTITLE)]
  Subtitle {
    /// Video: BV… or av… id, link (`?p=N` for a part), b23.tv short link or #N
    video: String,
    /// Track language, e.g. zh-CN, ai-zh, en
    #[arg(long)]
    lang: Option<String>,
  },
  /// AI summary and outline of a video
  Summary {
    /// Video: BV… or av… id, link (`?p=N` for a part), b23.tv short link or #N
    video: String,
  },
  /// Danmaku (bullet comments) of a video
  Danmaku {
    /// Video: BV… or av… id, link (`?p=N` for a part), b23.tv short link or #N
    video: String,
  },
  /// Videos related to a video
  Related {
    /// Video: BV… or av… id, link, b23.tv short link or #N
    video: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Ranking board: the whole site, or one region (see `hot --category`)
  #[command(after_long_help = EX_RANK)]
  Rank {
    /// One region's board instead of the whole site's
    #[arg(long, value_parser = clap::builder::PossibleValuesParser::new(videos::HOT_CATEGORIES))]
    region: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Your videos side by side with creator-center numbers (3-second bounce,
  /// share watched, follows, click-through rank ...)
  #[command(visible_alias = "my-videos", after_long_help = EX_VIDEO_STATS)]
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
    /// User: mid, `uid:MID`, space link, name or #N; yours when omitted
    user: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
}

const EX_COIN: &str = "\
Examples:
  atp bili coin BV1xx411c7mD
  atp bili coin BV1xx411c7mD -n 2 --like

More: atp guide bili coin";

const EX_SUBTITLE: &str = "\
Examples:
  atp bili subtitle BV1xx411c7mD
  atp bili subtitle 'https://www.bilibili.com/video/BV1xx411c7mD?p=2' --lang ai-zh

More: atp guide bili subtitle";

const EX_RANK: &str = "\
Examples:
  atp bili rank
  atp bili rank --region game -n 50";

const EX_VIDEO_STATS: &str = "\
Examples:
  atp bili video-stats
  atp bili video-stats -n 30 -f csv
  atp bili video-stats BV1xx411c7mD '#2'

More: atp guide bili insights";

async fn video_ref(api: &Api, arg: &str) -> Result<Video> {
  refs::video(api, &api.ctx.post_ref(arg)?).await
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

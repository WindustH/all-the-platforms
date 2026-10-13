//! TikTok-only commands: hashtags, sounds, playlists, related videos, and
//! the Marketing API (`ads`).

use atp_core::cli::PageArgs;
use atp_core::{Data, Page, Result};

use crate::api::Api;
use crate::{ads, users, videos};

const EX_ADS: &str = "\
Examples:
  atp tiktok login --token <access token>   # or TIKTOK_ADS_ACCESS_TOKEN
  atp tiktok ads advertisers
  atp tiktok ads use 7012345678901234567
  atp tiktok ads campaigns
  atp tiktok ads report --days 30 -f csv > report.csv";

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Videos of a hashtag
  Tag {
    /// `#name`, name or a /tag/ link
    tag: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Videos using a sound (or the sound itself with --info)
  Sound {
    /// Sound id or /music/ link
    sound: String,
    /// Show the sound instead of its videos
    #[arg(long)]
    info: bool,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Videos of a playlist (or the playlist itself with --info)
  Playlist {
    /// Playlist id or link
    playlist: String,
    #[arg(long)]
    info: bool,
    #[command(flatten)]
    page: PageArgs,
  },
  /// A creator's playlists (yours when USER is omitted)
  Playlists {
    user: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Videos TikTok recommends next to one
  Related {
    video: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// The Marketing API: ad accounts, campaigns, ads and reports
  #[command(after_long_help = EX_ADS)]
  Ads {
    #[command(subcommand)]
    command: ads::Command,
  },
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Tag { tag, page } => Data::Posts(
      page
        .collect_dated(async |r| videos::hashtag_videos(api, &tag, &r).await)
        .await?,
    ),
    Command::Sound {
      sound, info: true, ..
    } => Data::Collections(Page::last(vec![videos::sound(api, &sound).await?])),
    Command::Sound { sound, page, .. } => Data::Posts(
      page
        .collect_dated(async |r| videos::sound_videos(api, &sound, &r).await)
        .await?,
    ),
    Command::Playlist {
      playlist,
      info: true,
      ..
    } => Data::Collections(Page::last(vec![videos::playlist(api, &playlist).await?])),
    Command::Playlist { playlist, page, .. } => Data::Posts(
      page
        .collect_dated(async |r| videos::playlist_videos(api, &playlist, &r).await)
        .await?,
    ),
    Command::Playlists { user, page } => {
      let user = user.as_deref();
      Data::Collections(
        page
          .collect(async |r| users::playlists(api, user, &r).await)
          .await?,
      )
    }
    Command::Related { video, page } => Data::Posts(
      page
        .collect_dated(async |r| videos::related(api, &video, &r).await)
        .await?,
    ),
    Command::Ads { command } => ads::run(api, command).await?,
  })
}

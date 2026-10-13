//! YouTube-only commands: transcripts and caption tracks, playlists (and
//! managing them), a channel's other tabs and search, feeds with exact
//! times, related videos, hashtags, search suggestions, live chat, YouTube
//! Music, ratings of videos and comments, the bell, history removal, the
//! account's channels and the one-time OAuth grant for analytics.

mod channel;
mod manage;
mod music;
mod watch;

use social_core::cli::PageArgs;
use social_core::{Data, Result};

use self::music::MusicCommand;
use crate::api::Api;
use crate::channels::{self, Tab};
use crate::playlists;

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Transcript (captions) of a video
  #[command(visible_alias = "subtitle")]
  Transcript {
    video: String,
    /// Language code, e.g. en, de, zh-Hans; other languages are machine-translated
    #[arg(long)]
    lang: Option<String>,
  },
  /// Videos of a playlist
  Playlist {
    /// Playlist id or link
    playlist: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Shorts of a channel
  Shorts {
    user: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Live streams (past and upcoming) of a channel
  Streams {
    user: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Community posts of a channel
  #[command(visible_alias = "posts-tab")]
  Community {
    user: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Videos YouTube suggests next to a video
  Related {
    video: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Videos of a hashtag
  Hashtag {
    /// With or without `#`
    tag: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Grant read access to YouTube Analytics once (prints YOUTUBE_REFRESH_TOKEN)
  Oauth,
  /// Dislike a video (only you see it)
  Dislike {
    video: String,
    /// Remove the dislike instead
    #[arg(long)]
    undo: bool,
  },
  /// What the search box suggests for a prefix
  Suggest { query: String },
  /// Caption tracks of a video and the languages they translate into
  Captions { video: String },
  /// Chat of a stream: the replay of an ended one (in video order), or what
  /// a live one shows now (`listen` follows it)
  Chat {
    video: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Albums and singles of a channel (releases tab)
  Releases {
    user: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Podcasts of a channel
  Podcasts {
    user: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Search the videos of one channel
  ChannelSearch {
    user: String,
    query: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// The 15 latest uploads of a channel (or playlist) with exact times and views
  Rss {
    /// Channel, or a playlist with --playlist
    target: String,
    #[arg(long)]
    playlist: bool,
  },
  /// Channels of the logged-in account (its own and brand accounts)
  Accounts,
  /// YouTube Music: search, songs of an album / artist / playlist, lyrics
  Music {
    #[command(subcommand)]
    command: MusicCommand,
  },
  /// Like a comment (or dislike it with --dislike); only you see dislikes
  LikeComment {
    video: String,
    comment: String,
    #[arg(long)]
    dislike: bool,
    /// Take the like or dislike back
    #[arg(long)]
    undo: bool,
  },
  /// Notifications for a subscribed channel: all, personalized or none
  Bell {
    user: String,
    #[arg(value_parser = clap::builder::PossibleValuesParser::new(channels::BELL))]
    level: String,
  },
  /// Create a playlist, optionally with videos
  PlaylistCreate {
    title: String,
    videos: Vec<String>,
    #[arg(long)]
    description: Option<String>,
    #[arg(long, default_value = "private", value_parser = clap::builder::PossibleValuesParser::new(playlists::PRIVACY))]
    privacy: String,
  },
  /// Delete one of your playlists
  PlaylistDelete {
    playlist: String,
    /// Confirm the deletion
    #[arg(short, long)]
    yes: bool,
  },
  /// Add videos to one of your playlists
  PlaylistAdd {
    playlist: String,
    videos: Vec<String>,
  },
  /// Remove videos from one of your playlists
  PlaylistRemove {
    playlist: String,
    videos: Vec<String>,
  },
  /// Rename a playlist, change its description or privacy
  PlaylistEdit {
    playlist: String,
    #[arg(long)]
    title: Option<String>,
    #[arg(long)]
    description: Option<String>,
    #[arg(long, value_parser = clap::builder::PossibleValuesParser::new(playlists::PRIVACY))]
    privacy: Option<String>,
  },
  /// Save someone's playlist to your library (or remove it with --undo)
  PlaylistSave {
    playlist: String,
    #[arg(long)]
    undo: bool,
  },
  /// Remove a video from your watch history
  HistoryRemove {
    video: String,
    /// History pages to look through
    #[arg(long, default_value_t = 5)]
    pages: usize,
  },
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  use Command as C;
  match command {
    C::Transcript { video, lang } => watch::transcript(api, &video, lang.as_deref()).await,
    C::Playlist { playlist, page } => watch::playlist(api, &playlist, &page).await,
    C::Shorts { user, page } => channel::tab(api, &user, Tab::Shorts, &page).await,
    C::Streams { user, page } => channel::tab(api, &user, Tab::Streams, &page).await,
    C::Community { user, page } => channel::tab(api, &user, Tab::Posts, &page).await,
    C::Related { video, page } => watch::related(api, &video, &page).await,
    C::Hashtag { tag, page } => watch::hashtag(api, &tag, &page).await,
    C::Oauth => manage::oauth(api).await,
    C::Dislike { video, undo } => manage::dislike(api, &video, undo).await,
    C::Suggest { query } => watch::suggest(api, &query).await,
    C::Captions { video } => watch::captions(api, &video).await,
    C::Chat { video, page } => watch::chat(api, &video, &page).await,
    C::Releases { user, page } => channel::shelves(api, &user, Tab::Releases, &page).await,
    C::Podcasts { user, page } => channel::shelves(api, &user, Tab::Podcasts, &page).await,
    C::ChannelSearch { user, query, page } => channel::search(api, &user, &query, &page).await,
    C::Rss { target, playlist } => channel::rss(api, &target, playlist).await,
    C::Accounts => manage::accounts(api).await,
    C::Music { command } => music::run(api, command).await,
    C::LikeComment {
      video,
      comment,
      dislike,
      undo,
    } => manage::rate_comment(api, &video, &comment, dislike, undo).await,
    C::Bell { user, level } => manage::bell(api, &user, &level).await,
    C::PlaylistCreate {
      title,
      videos,
      description,
      privacy,
    } => manage::create_playlist(api, &title, &videos, description.as_deref(), &privacy).await,
    C::PlaylistDelete { playlist, yes } => manage::delete_playlist(api, &playlist, yes).await,
    C::PlaylistAdd { playlist, videos } => manage::add_to_playlist(api, &playlist, &videos).await,
    C::PlaylistRemove { playlist, videos } => {
      manage::remove_from_playlist(api, &playlist, &videos).await
    }
    C::PlaylistEdit {
      playlist,
      title,
      description,
      privacy,
    } => {
      let changes = manage::PlaylistChanges {
        title: title.as_deref(),
        description: description.as_deref(),
        privacy: privacy.as_deref(),
      };
      manage::edit_playlist(api, &playlist, &changes).await
    }
    C::PlaylistSave { playlist, undo } => manage::save_playlist(api, &playlist, undo).await,
    C::HistoryRemove { video, pages } => manage::forget(api, &video, pages).await,
  }
}

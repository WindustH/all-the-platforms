//! YouTube-only commands: transcripts and caption tracks, playlists (their
//! order, privacy and saving them; creating and filling them are the shared
//! `folder-*` commands), a channel's other tabs and search, feeds with exact
//! times, related videos, hashtags, search suggestions, live chat, YouTube
//! Music, ratings of videos and comments, the bell, clearing the search
//! history, the account's channels and the one-time OAuth grant for
//! analytics.

mod channel;
mod manage;
mod music;
mod watch;

use atp_core::cli::PageArgs;
use atp_core::{Data, Result};

use self::music::MusicCommand;
use crate::api::Api;
use crate::channels::{self, Tab};
use crate::playlists;

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Transcript (captions) of a video
  #[command(visible_alias = "subtitle", after_long_help = EX_TRANSCRIPT)]
  Transcript {
    /// Video: id, link or #N
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
    /// Channel: @handle, UC… id, link or #N
    user: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Live streams (past and upcoming) of a channel
  Streams {
    /// Channel: @handle, UC… id, link or #N
    user: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Community posts of a channel
  #[command(visible_alias = "posts-tab")]
  Community {
    /// Channel: @handle, UC… id, link or #N
    user: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Videos YouTube suggests next to a video
  Related {
    /// Video: id, link or #N
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
  /// Grant access to YouTube Analytics and video edits once (prints YOUTUBE_REFRESH_TOKEN)
  #[command(after_long_help = EX_OAUTH)]
  Oauth,
  /// Dislike a video (only you see it)
  Dislike {
    /// Video: id, link or #N
    video: String,
    /// Remove the dislike instead
    #[arg(long)]
    undo: bool,
  },
  /// What the search box suggests for a prefix
  Suggest {
    /// The start of a search
    query: String,
  },
  /// Caption tracks of a video and the languages they translate into
  Captions {
    /// Video: id, link or #N
    video: String,
  },
  /// Chat of a stream: the replay of an ended one (in video order), or what
  /// a live one shows now (`listen` follows it)
  Chat {
    /// Stream: video id, link or #N
    video: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Albums and singles of a channel (releases tab)
  Releases {
    /// Channel: @handle, UC… id, link or #N
    user: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Podcasts of a channel
  Podcasts {
    /// Channel: @handle, UC… id, link or #N
    user: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Search the videos of one channel
  #[command(after_long_help = EX_CHANNEL_SEARCH)]
  ChannelSearch {
    /// Channel: @handle, UC… id, link or #N
    user: String,
    /// What to search for
    query: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// The 15 latest uploads of a channel (or playlist) with exact times and views
  #[command(after_long_help = EX_RSS)]
  Rss {
    /// Channel (@handle, UC… id, link or #N), or a playlist id or link with --playlist
    target: String,
    /// Read the feed of a playlist instead of a channel
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
  #[command(after_long_help = EX_LIKE_COMMENT)]
  LikeComment {
    /// Video the comment is under: id, link or #N
    video: String,
    /// Comment: id or link with `lc=`
    comment: String,
    /// Dislike the comment instead
    #[arg(long)]
    dislike: bool,
    /// Take the like or dislike back
    #[arg(long)]
    undo: bool,
  },
  /// Notifications for a subscribed channel: all, personalized or none
  #[command(after_long_help = EX_BELL)]
  Bell {
    /// Channel: @handle, UC… id, link or #N
    user: String,
    /// Notification setting
    #[arg(value_parser = clap::builder::PossibleValuesParser::new(channels::BELL))]
    level: String,
  },
  /// Move a video within one of your playlists: after another one, or to the top
  #[command(after_long_help = EX_PLAYLIST_MOVE)]
  PlaylistMove {
    /// Your playlist: id, link or #N after `collections`
    playlist: String,
    /// Video to move: id, link or #N
    video: String,
    /// Put it right after this video of the playlist (id, link or #N)
    #[arg(long, value_name = "VIDEO", conflicts_with = "top")]
    after: Option<String>,
    /// Put it first
    #[arg(long, required_unless_present = "after")]
    top: bool,
  },
  /// Make one of your playlists private, unlisted or public
  #[command(after_long_help = EX_PLAYLIST_PRIVACY)]
  PlaylistPrivacy {
    /// Your playlist: id, link or #N after `collections`
    playlist: String,
    /// Who can see it
    #[arg(value_parser = clap::builder::PossibleValuesParser::new(playlists::PRIVACY))]
    level: String,
  },
  /// Save someone's playlist to your library (or remove it with --undo)
  PlaylistSave {
    /// Playlist: id, link or #N
    playlist: String,
    /// Remove it from your library instead
    #[arg(long)]
    undo: bool,
  },
  /// Clear your whole search history (`history-clear` clears the watch history)
  #[command(after_long_help = EX_SEARCH_HISTORY_CLEAR)]
  SearchHistoryClear {
    /// Skip the confirmation question (needed in scripts)
    #[arg(short, long)]
    yes: bool,
  },
}

const EX_TRANSCRIPT: &str = "\
Examples:
  atp yt transcript dQw4w9WgXcQ
  atp yt transcript https://youtu.be/dQw4w9WgXcQ --lang de -f json
  atp yt captions dQw4w9WgXcQ          # the tracks and languages first

More: atp guide yt transcript";

const EX_OAUTH: &str = "\
Examples:
  export YOUTUBE_CLIENT_ID=... YOUTUBE_CLIENT_SECRET=...
  atp yt oauth                         # open the URL, allow access
  export YOUTUBE_REFRESH_TOKEN=...     # the refresh_token it prints
  atp yt insights --days 28

More: atp guide yt login";

const EX_CHANNEL_SEARCH: &str = "\
Examples:
  atp yt channel-search @NASA \"artemis\"
  atp yt channel-search https://www.youtube.com/@NASA moon -n 50";

const EX_RSS: &str = "\
Examples:
  atp yt rss @NASA
  atp yt rss 'https://www.youtube.com/playlist?list=PLBCF2DAC6FFB574DE' --playlist";

const EX_LIKE_COMMENT: &str = "\
Examples:
  atp yt like-comment dQw4w9WgXcQ UgzsIxQWD8kS3Me08tR4AaABAg
  atp yt like-comment dQw4w9WgXcQ UgzsIxQWD8kS3Me08tR4AaABAg --dislike
  atp yt like-comment dQw4w9WgXcQ UgzsIxQWD8kS3Me08tR4AaABAg --undo";

const EX_BELL: &str = "\
Examples:
  atp yt bell @NASA all
  atp yt bell @NASA none";

const EX_PLAYLIST_MOVE: &str = "\
Examples:
  atp yt collections                   # your playlists, numbered
  atp yt favorites --folder '#2' -n 100
  atp yt playlist-move '#2' '#7' --after '#3'
  atp yt playlist-move PLBCF2DAC6FFB574DE dQw4w9WgXcQ --top

More: atp guide yt playlist-move";

const EX_PLAYLIST_PRIVACY: &str = "\
Examples:
  atp yt playlist-privacy '#1' unlisted
  atp yt playlist-privacy PLBCF2DAC6FFB574DE public

`folder-edit --private` / `--public` do the same without unlisted.";

const EX_SEARCH_HISTORY_CLEAR: &str = "\
Examples:
  atp yt search-history-clear
  atp yt search-history-clear -y
  atp yt history-clear -y              # the watch history

It cannot be undone. More: atp guide yt search-history-clear";

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
    C::PlaylistMove {
      playlist,
      video,
      after,
      top: _,
    } => manage::move_in_playlist(api, &playlist, &video, after.as_deref()).await,
    C::PlaylistPrivacy { playlist, level } => {
      manage::playlist_privacy(api, &playlist, &level).await
    }
    C::PlaylistSave { playlist, undo } => manage::save_playlist(api, &playlist, undo).await,
    C::SearchHistoryClear { yes } => manage::clear_search_history(api, yes).await,
  }
}

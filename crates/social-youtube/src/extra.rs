//! YouTube-only commands: transcripts and caption tracks, playlists (and
//! managing them), a channel's other tabs and search, feeds with exact
//! times, related videos, hashtags, search suggestions, live chat, YouTube
//! Music, ratings of videos and comments, the bell, history removal, the
//! account's channels and the one-time OAuth grant for analytics.

use social_core::cli::PageArgs;
use social_core::{Action, Data, Error, Page, Result, json};

use crate::api::Api;
use crate::channels::{self, Tab};
use crate::music::{self, Found};
use crate::{
  account, captions, comments, insights, library, live, playlists, refs, rss, search, videos,
};

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

#[derive(Debug, clap::Subcommand)]
pub enum MusicCommand {
  /// Search YouTube Music
  Search {
    query: String,
    /// What to look for
    #[arg(short = 't', long = "type", default_value = "songs", value_parser = clap::builder::PossibleValuesParser::new(music::KINDS.iter().map(|(k, _)| *k)))]
    kind: String,
  },
  /// Songs of an album (MPREb…), artist (UC…) or playlist
  Tracks { id: String },
  /// Lyrics of a song
  Lyrics { video: String },
}

async fn shelves(api: &Api, user: &str, tab: Tab, page: &PageArgs) -> Result<Data> {
  let user = api.ctx.user_ref(user)?;
  Ok(Data::Collections(
    page
      .collect(async |r| {
        let l = channels::tab(api, &user, tab, &r).await?;
        Ok(Page::new(l.collections, l.next))
      })
      .await?,
  ))
}

async fn tab(api: &Api, user: &str, tab: Tab, page: &PageArgs) -> Result<Data> {
  let user = api.ctx.user_ref(user)?;
  Ok(Data::Posts(
    page
      .collect_dated(async |r| {
        let l = channels::tab(api, &user, tab, &r).await?;
        Ok(social_core::Page::new(l.posts, l.next))
      })
      .await?,
  ))
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  use Command as C;
  let ctx = &api.ctx;
  Ok(match command {
    C::Transcript { video, lang } => {
      Data::Transcript(captions::transcript(api, &ctx.post_ref(&video)?, lang.as_deref()).await?)
    }
    C::Playlist { playlist, page } => {
      let id = refs::playlist(&playlist)?;
      Data::Posts(
        page
          .collect_dated(async |r| library::playlist(api, &id, &r).await)
          .await?,
      )
    }
    C::Shorts { user, page } => tab(api, &user, Tab::Shorts, &page).await?,
    C::Streams { user, page } => tab(api, &user, Tab::Streams, &page).await?,
    C::Community { user, page } => tab(api, &user, Tab::Posts, &page).await?,
    C::Related { video, page } => {
      let v = ctx.post_ref(&video)?;
      Data::Posts(
        page
          .collect_dated(async |r| videos::related(api, &v, &r).await)
          .await?,
      )
    }
    C::Hashtag { tag, page } => Data::Posts(
      page
        .collect_dated(async |r| videos::hashtag(api, &tag, &r).await)
        .await?,
    ),
    C::Oauth => Data::Value(insights::authorize(ctx).await?),
    C::Dislike { video, undo } => {
      let id = refs::video(&ctx.post_ref(&video)?)?;
      let (path, name) = if undo {
        ("like/removelike", "undislike")
      } else {
        ("like/dislike", "dislike")
      };
      api
        .write(path, json!({ "target": { "videoId": id } }))
        .await?;
      Data::Action(Action::done(name, &id).with_url(refs::video_url(&id)))
    }
    C::Suggest { query } => Data::Value(json!(search::suggest(api, &query).await?)),
    C::Captions { video } => Data::Value(captions::tracks(api, &ctx.post_ref(&video)?).await?),
    C::Chat { video, page } => {
      let v = ctx.post_ref(&video)?;
      Data::Posts(
        page
          .collect(async |r| live::page(api, &v, &r).await)
          .await?,
      )
    }
    C::Releases { user, page } => shelves(api, &user, Tab::Releases, &page).await?,
    C::Podcasts { user, page } => shelves(api, &user, Tab::Podcasts, &page).await?,
    C::ChannelSearch { user, query, page } => {
      let user = ctx.user_ref(&user)?;
      Data::Posts(
        page
          .collect(async |r| {
            let l = channels::search(api, &user, &query, &r).await?;
            Ok(Page::new(l.posts, l.next))
          })
          .await?,
      )
    }
    C::Rss { target, playlist } => {
      let target = if playlist {
        target
      } else {
        ctx.user_ref(&target)?
      };
      Data::Posts(Page::last(rss::feed(api, &target, playlist).await?))
    }
    C::Accounts => Data::Users(Page::last(account::accounts(api).await?)),
    C::Music { command } => music_command(api, command).await?,
    C::LikeComment {
      video,
      comment: target,
      dislike,
      undo,
    } => Data::Action(comments::rate(api, &ctx.post_ref(&video)?, &target, dislike, undo).await?),
    C::Bell { user, level } => {
      Data::Action(channels::bell(api, &ctx.user_ref(&user)?, &level).await?)
    }
    C::PlaylistCreate {
      title,
      videos,
      description,
      privacy,
    } => {
      let videos = refs_of(api, &videos)?;
      Data::Action(playlists::create(api, &title, description.as_deref(), &privacy, &videos).await?)
    }
    C::PlaylistDelete { playlist, yes } => {
      if !yes {
        return Err(Error::input(format!(
          "refusing to delete playlist {playlist} without --yes"
        )));
      }
      Data::Action(playlists::delete(api, &ctx.collection_ref(&playlist)?).await?)
    }
    C::PlaylistAdd { playlist, videos } => {
      let videos = refs_of(api, &videos)?;
      Data::Action(playlists::add(api, &ctx.collection_ref(&playlist)?, &videos).await?)
    }
    C::PlaylistRemove { playlist, videos } => {
      let videos = refs_of(api, &videos)?;
      Data::Action(playlists::remove(api, &ctx.collection_ref(&playlist)?, &videos).await?)
    }
    C::PlaylistEdit {
      playlist,
      title,
      description,
      privacy,
    } => Data::Action(
      playlists::update(
        api,
        &ctx.collection_ref(&playlist)?,
        title.as_deref(),
        description.as_deref(),
        privacy.as_deref(),
      )
      .await?,
    ),
    C::PlaylistSave { playlist, undo } => {
      Data::Action(playlists::save(api, &ctx.collection_ref(&playlist)?, undo).await?)
    }
    C::HistoryRemove { video, pages } => {
      Data::Action(library::forget(api, &ctx.post_ref(&video)?, pages).await?)
    }
  })
}

/// Video arguments with `#N` resolved.
fn refs_of(api: &Api, videos: &[String]) -> Result<Vec<String>> {
  videos.iter().map(|v| api.ctx.post_ref(v)).collect()
}

async fn music_command(api: &Api, command: MusicCommand) -> Result<Data> {
  Ok(match command {
    MusicCommand::Search { query, kind } => match music::search(api, &query, &kind).await? {
      Found::Songs(p) => Data::Posts(Page::last(p)),
      Found::Albums(c) => Data::Collections(Page::last(c)),
      Found::Artists(u) => Data::Users(Page::last(u)),
    },
    MusicCommand::Tracks { id } => {
      let id = api.ctx.collection_ref(&id)?;
      Data::Posts(Page::last(music::tracks(api, &id).await?))
    }
    MusicCommand::Lyrics { video } => {
      let id = refs::video(&api.ctx.post_ref(&video)?)?;
      Data::Value(music::lyrics(api, &id).await?)
    }
  })
}

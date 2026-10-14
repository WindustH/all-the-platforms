//! NetEase-only commands: your playlists, lyrics, the cloud drive, the
//! charts, the listening ranking and the daily check-in.

use std::path::PathBuf;

use atp_core::cli::PageArgs;
use atp_core::{Action, Data, Error, Result};

use crate::api::Api;
use crate::refs::{self, PostRef};
use crate::uploads::Tags;
use crate::{cloud, discover, library, lyrics, playlists, uploads, users};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Your playlists: create, add or remove songs, delete; any playlist's songs
  #[command(subcommand)]
  Playlist(PlaylistCommand),
  /// Lyrics of a song: plain text with translations under their lines, or LRC
  #[command(after_long_help = EX_LYRICS)]
  Lyrics {
    song: String,
    /// LRC with times instead of plain text
    #[arg(long)]
    lrc: bool,
    /// Leave the translation out
    #[arg(long)]
    original: bool,
  },
  /// Your music cloud drive (云盘): your own uploaded files
  #[command(subcommand)]
  Cloud(CloudCommand),
  /// Every chart (榜单), as playlists to read
  Toplists {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Listening ranking (听歌排行) of a user, yours without USER
  Ranking {
    user: Option<String>,
    /// All time instead of the last week
    #[arg(long)]
    all_time: bool,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Daily check-in (每日签到)
  Signin,
}

#[derive(Debug, clap::Subcommand)]
pub enum PlaylistCommand {
  /// Create a playlist
  Create {
    name: String,
    /// Only you see it
    #[arg(long)]
    private: bool,
  },
  /// Add songs to one of your playlists
  Add {
    playlist: String,
    #[arg(value_name = "SONG", required = true)]
    songs: Vec<String>,
  },
  /// Remove songs from one of your playlists
  Remove {
    playlist: String,
    #[arg(value_name = "SONG", required = true)]
    songs: Vec<String>,
  },
  /// Delete one of your playlists
  Delete {
    playlist: String,
    #[arg(short, long)]
    yes: bool,
  },
  /// The songs of a playlist (or a chart), page by page
  Tracks {
    playlist: String,
    #[command(flatten)]
    page: PageArgs,
  },
}

#[derive(Debug, clap::Subcommand)]
pub enum CloudCommand {
  /// Your files, newest first
  Ls {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Upload your own audio files (at once when the drive holds them already)
  Upload {
    #[arg(required = true)]
    files: Vec<PathBuf>,
    /// Artist the drive shows for them
    #[arg(long)]
    artist: Option<String>,
    /// Album the drive shows for them
    #[arg(long)]
    album: Option<String>,
  },
  /// Download files you uploaded, by their ids from `cloud ls`
  Download {
    #[arg(value_name = "FILE", required = true)]
    files: Vec<String>,
    /// Directory to save into
    #[arg(short = 'o', long, default_value = ".")]
    dir: PathBuf,
  },
  /// Delete files from your cloud drive
  Delete {
    #[arg(value_name = "FILE", required = true)]
    files: Vec<String>,
    #[arg(short, long)]
    yes: bool,
  },
  /// Space used and allowed
  Quota,
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Playlist(command) => playlist(api, command).await?,
    Command::Lyrics {
      song,
      lrc,
      original,
    } => {
      let id = song_id(api, &song).await?;
      Data::Post(Box::new(lyrics::command(api, id, lrc, original).await?))
    }
    Command::Cloud(command) => cloud(api, command).await?,
    Command::Toplists { page } => Data::Posts(
      page
        .collect_dated(async |r| discover::toplists(api, &r).await)
        .await?,
    ),
    Command::Ranking {
      user,
      all_time,
      page,
    } => {
      let user = user.map(|u| api.ctx.user_ref(&u)).transpose()?;
      let user = user.as_deref();
      Data::Posts(
        page
          .collect_dated(async |r| library::ranking(api, user, all_time, &r).await)
          .await?,
      )
    }
    Command::Signin => Data::Action(users::signin(api).await?),
  })
}

async fn playlist(api: &Api, command: PlaylistCommand) -> Result<Data> {
  use PlaylistCommand as P;
  let action = match command {
    P::Create { name, private } => playlists::create(api, &name, private).await?,
    P::Add { playlist, songs } => change(api, &playlist, &songs, false).await?,
    P::Remove { playlist, songs } => change(api, &playlist, &songs, true).await?,
    P::Delete { playlist, yes } => {
      playlists::delete(api, playlist_id(api, &playlist).await?, yes).await?
    }
    P::Tracks { playlist, page } => {
      let id = playlist_id(api, &playlist).await?;
      return Ok(Data::Posts(
        page
          .collect_dated(async |r| playlists::tracks(api, id, &r).await)
          .await?,
      ));
    }
  };
  Ok(Data::Action(action))
}

async fn change(api: &Api, playlist: &str, songs: &[String], remove: bool) -> Result<Action> {
  let id = playlist_id(api, playlist).await?;
  let mut refs = Vec::with_capacity(songs.len());
  for song in songs {
    refs.push(PostRef::Song(song_id(api, song).await?));
  }
  playlists::change(api, id, &refs, remove).await
}

async fn cloud(api: &Api, command: CloudCommand) -> Result<Data> {
  use CloudCommand as C;
  let resolve = |args: Vec<String>| {
    args
      .iter()
      .map(|a| api.ctx.post_ref(a))
      .collect::<Result<Vec<_>>>()
  };
  Ok(match command {
    C::Ls { page } => Data::Files(page.collect(async |r| cloud::list(api, &r).await).await?),
    C::Upload {
      files,
      artist,
      album,
    } => uploads::upload(api, &files, &Tags { artist, album }).await?,
    C::Download { files, dir } => cloud::download(api, &resolve(files)?, &dir).await?,
    C::Delete { files, yes } => Data::Action(cloud::delete(api, &resolve(files)?, yes).await?),
    C::Quota => cloud::quota(api).await?,
  })
}

/// A song argument (`#N`, id or link) as its id.
async fn song_id(api: &Api, arg: &str) -> Result<u64> {
  match refs::post(api, &api.ctx.post_ref(arg)?).await? {
    PostRef::Song(id) => Ok(id),
    other => Err(Error::input(format!("not a song: {other:?}"))),
  }
}

/// A playlist argument (`#N`, `playlist:ID` or a link) as its id; a bare
/// number is taken as a playlist id here.
async fn playlist_id(api: &Api, arg: &str) -> Result<u64> {
  let arg = api.ctx.post_ref(arg)?;
  if let Ok(id) = arg.trim().parse() {
    return Ok(id);
  }
  match refs::post(api, &arg).await? {
    PostRef::Playlist(id) => Ok(id),
    other => Err(Error::input(format!("not a playlist: {other:?}"))),
  }
}

const EX_LYRICS: &str = "\
Examples:
  atp ncm lyrics 186016
  atp ncm lyrics '#1' --lrc --json | jq -r .data.text > song.lrc
  atp ncm lyrics https://music.163.com/song?id=186016 --original";

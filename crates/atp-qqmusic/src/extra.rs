//! QQ Music-only commands: lyrics, the account's playlists and any
//! playlist's or chart's songs, and the list of charts.

use atp_core::cli::PageArgs;
use atp_core::{Data, Error, Result};

use crate::api::Api;
use crate::refs::{self, Key, PostRef};
use crate::{discover, lyrics, playlists};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Your playlists: create, add or remove songs, delete; any playlist's or chart's songs
  #[command(subcommand, after_long_help = EX_PLAYLIST)]
  Playlist(PlaylistCommand),
  /// Lyrics of a song: plain text with translations under their lines, or LRC
  #[command(after_long_help = EX_LYRICS)]
  Lyrics {
    /// Song: mid, number, `song:MID`, link or #N
    song: String,
    /// LRC with times instead of plain text
    #[arg(long)]
    lrc: bool,
    /// Leave the translation out
    #[arg(long)]
    original: bool,
  },
  /// Every chart (榜单), to read or list with `playlist tracks`
  #[command(after_long_help = EX_TOPLISTS)]
  Toplists {
    #[command(flatten)]
    page: PageArgs,
  },
}

#[derive(Debug, clap::Subcommand)]
pub enum PlaylistCommand {
  /// Create a playlist
  #[command(after_long_help = EX_PLAYLIST_CREATE)]
  Create {
    /// Name of the new playlist (at most 20 Chinese characters or 40 letters)
    name: String,
  },
  /// Add songs to one of your playlists
  #[command(after_long_help = EX_PLAYLIST_ADD)]
  Add {
    /// Playlist: id, `playlist:ID`, link or #N
    playlist: String,
    /// Songs: mids, numbers, `song:MID`, links or #N
    #[arg(value_name = "SONG", required = true)]
    songs: Vec<String>,
  },
  /// Remove songs from one of your playlists
  #[command(after_long_help = EX_PLAYLIST_REMOVE)]
  Remove {
    /// Playlist: id, `playlist:ID`, link or #N
    playlist: String,
    /// Songs: mids, numbers, `song:MID`, links or #N
    #[arg(value_name = "SONG", required = true)]
    songs: Vec<String>,
  },
  /// Delete one of your playlists
  #[command(after_long_help = EX_PLAYLIST_DELETE)]
  Delete {
    /// Playlist: id, `playlist:ID`, link or #N
    playlist: String,
    /// Skip the confirmation question (needed in scripts)
    #[arg(short, long)]
    yes: bool,
  },
  /// The songs of a playlist or a chart, page by page
  #[command(after_long_help = EX_PLAYLIST_TRACKS)]
  Tracks {
    /// Playlist or chart: id, `playlist:ID`, `toplist:ID`, link or #N
    playlist: String,
    #[command(flatten)]
    page: PageArgs,
  },
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Playlist(command) => playlist(api, command).await?,
    Command::Lyrics {
      song,
      lrc,
      original,
    } => {
      let key = song_key(api, &song).await?;
      Data::Post(Box::new(lyrics::command(api, &key, lrc, original).await?))
    }
    Command::Toplists { page } => Data::Posts(
      page
        .collect_dated(async |r| discover::toplists(api, &r).await)
        .await?,
    ),
  })
}

async fn playlist(api: &Api, command: PlaylistCommand) -> Result<Data> {
  use PlaylistCommand as P;
  let action = match command {
    P::Create { name } => playlists::create(api, &name).await?,
    P::Add { playlist, songs } => change(api, &playlist, &songs, false).await?,
    P::Remove { playlist, songs } => change(api, &playlist, &songs, true).await?,
    P::Delete { playlist, yes } => {
      playlists::delete(api, playlist_id(api, &playlist).await?, yes).await?
    }
    P::Tracks { playlist, page } => return tracks(api, &playlist, page).await,
  };
  Ok(Data::Action(action))
}

/// The songs of a playlist or a chart.
async fn tracks(api: &Api, arg: &str, page: PageArgs) -> Result<Data> {
  let arg = api.ctx.post_ref(arg)?;
  let target = match arg.trim().parse() {
    Ok(id) => PostRef::Playlist(id),
    Err(_) => refs::post(api, &arg).await?,
  };
  let posts = match target {
    PostRef::Playlist(id) => {
      page
        .collect_dated(async |r| playlists::tracks(api, id, &r).await)
        .await?
    }
    PostRef::Toplist(id) => {
      page
        .collect_dated(async |r| discover::tracks(api, id, &r).await)
        .await?
    }
    other => {
      return Err(Error::input(format!(
        "not a playlist or a chart: {other:?}"
      )));
    }
  };
  Ok(Data::Posts(posts))
}

async fn change(
  api: &Api,
  playlist: &str,
  songs: &[String],
  remove: bool,
) -> Result<atp_core::Action> {
  api.require_login()?;
  let id = playlist_id(api, playlist).await?;
  let mut keys = Vec::with_capacity(songs.len());
  for song in songs {
    keys.push(song_key(api, song).await?);
  }
  playlists::change(api, id, &keys, remove).await
}

/// A song argument (`#N`, mid, number or link) as its key.
async fn song_key(api: &Api, arg: &str) -> Result<Key> {
  match refs::post(api, &api.ctx.post_ref(arg)?).await? {
    PostRef::Song(key) => Ok(key),
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
  atp qqmusic lyrics 0039MnYb0qxYhV
  atp qqmusic lyrics '#1' --lrc --json | jq -r .data.text > song.lrc
  atp qqmusic lyrics https://y.qq.com/n/ryqq_v2/songDetail/002sKSzt2E3aCy --original

More: atp guide qqmusic lyrics";

const EX_TOPLISTS: &str = "\
Examples:
  atp qqmusic toplists
  atp qqmusic playlist tracks toplist:62 --limit 10

More: atp guide qqmusic toplists";

const EX_PLAYLIST: &str = "\
Examples:
  atp qqmusic playlist create \"Road trip\"
  atp qqmusic playlist add playlist:8523075134 0039MnYb0qxYhV '#2'
  atp qqmusic playlist tracks toplist:26

More: atp guide qqmusic playlist";

const EX_PLAYLIST_CREATE: &str = "\
Examples:
  atp qqmusic playlist create \"Road trip\"
  atp qqmusic playlist create 通勤

More: atp guide qqmusic playlist create";

const EX_PLAYLIST_ADD: &str = "\
Examples:
  atp qqmusic playlist add 8523075134 0039MnYb0qxYhV 002sKSzt2E3aCy
  atp qqmusic playlist add playlist:8523075134 '#1'   # first song of the last list

More: atp guide qqmusic playlist add";

const EX_PLAYLIST_REMOVE: &str = "\
Examples:
  atp qqmusic playlist remove 8523075134 0039MnYb0qxYhV
  atp qqmusic playlist remove playlist:8523075134 '#3'

More: atp guide qqmusic playlist remove";

const EX_PLAYLIST_DELETE: &str = "\
Examples:
  atp qqmusic playlist delete 8523075134
  atp qqmusic playlist delete playlist:8523075134 --yes

More: atp guide qqmusic playlist delete";

const EX_PLAYLIST_TRACKS: &str = "\
Examples:
  atp qqmusic playlist tracks 8523075134 --limit 50
  atp qqmusic playlist tracks toplist:62
  atp qqmusic playlist tracks https://y.qq.com/n/ryqq_v2/playlist/8523075134

More: atp guide qqmusic playlist tracks";

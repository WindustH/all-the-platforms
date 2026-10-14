//! `playlist create|add|remove|delete|tracks`.

use atp_core::cli::PageArgs;
use atp_core::{Action, Data, Result};

use crate::api::Api;
use crate::playlists::{self, New};
use crate::refs::{self, Kind};

#[derive(Debug, clap::Subcommand)]
pub enum PlaylistCommand {
  /// Create a playlist (public unless --private or --collaborative)
  #[command(after_long_help = EX_CREATE)]
  Create {
    /// Name of the new playlist
    name: String,
    /// Description shown under the name
    #[arg(short, long)]
    description: Option<String>,
    /// Make it private
    #[arg(long)]
    private: bool,
    /// Let people you invite add to it (Spotify makes it private)
    #[arg(long)]
    collaborative: bool,
  },
  /// Add tracks or episodes to the end of one of your playlists
  #[command(after_long_help = EX_ADD)]
  Add {
    /// Playlist: link, spotify: URI, `playlist:ID`, id or #N
    playlist: String,
    /// Tracks or episodes: links, spotify: URIs, `episode:ID`, track ids or #N
    #[arg(value_name = "ITEM", required = true)]
    items: Vec<String>,
  },
  /// Remove tracks or episodes (every time they occur) from one of your playlists
  #[command(after_long_help = EX_REMOVE)]
  Remove {
    /// Playlist: link, spotify: URI, `playlist:ID`, id or #N
    playlist: String,
    /// Tracks or episodes: links, spotify: URIs, `episode:ID`, track ids or #N
    #[arg(value_name = "ITEM", required = true)]
    items: Vec<String>,
  },
  /// Delete one of your playlists (Spotify unfollows it)
  #[command(after_long_help = EX_DELETE)]
  Delete {
    /// Playlist: link, spotify: URI, `playlist:ID`, id or #N
    playlist: String,
    /// Skip the confirmation question (needed in scripts)
    #[arg(short, long)]
    yes: bool,
  },
  /// The items of one of your own or collaborative playlists, page by page
  #[command(after_long_help = EX_TRACKS)]
  Tracks {
    /// Playlist: link, spotify: URI, `playlist:ID`, id or #N
    playlist: String,
    #[command(flatten)]
    page: PageArgs,
  },
}

pub async fn run(api: &Api, command: PlaylistCommand) -> Result<Data> {
  use PlaylistCommand as P;
  let action = match command {
    P::Create {
      name,
      description,
      private,
      collaborative,
    } => {
      let new = New {
        name,
        description,
        private,
        collaborative,
      };
      playlists::create(api, new).await?
    }
    P::Add { playlist, items } => change(api, &playlist, &items, false).await?,
    P::Remove { playlist, items } => change(api, &playlist, &items, true).await?,
    P::Delete { playlist, yes } => playlists::delete(api, &id(api, &playlist).await?, yes).await?,
    P::Tracks { playlist, page } => {
      let id = id(api, &playlist).await?;
      let posts = page
        .collect_dated(async |r| playlists::tracks(api, &id, &r).await)
        .await?;
      return Ok(Data::Posts(posts));
    }
  };
  Ok(Data::Action(action))
}

async fn change(api: &Api, playlist: &str, items: &[String], remove: bool) -> Result<Action> {
  let id = id(api, playlist).await?;
  let kinds = [Kind::Track, Kind::Episode];
  let mut found = Vec::with_capacity(items.len());
  for arg in items {
    let arg = api.ctx.post_ref(arg)?;
    found.push(refs::expect(api, &arg, Kind::Track, &kinds).await?);
  }
  playlists::change(api, &id, &found, remove).await
}

/// A playlist argument (`#N`, link, URI, `playlist:ID` or a bare id) as its id.
async fn id(api: &Api, arg: &str) -> Result<String> {
  let arg = api.ctx.post_ref(arg)?;
  let item = refs::expect(api, &arg, Kind::Playlist, &[Kind::Playlist]).await?;
  Ok(item.id)
}

pub const EX: &str = "\
Examples:
  atp spotify playlist create \"Road trip\" --private
  atp spotify playlist add 3cEYpjA9oz9GiPac4AsH4n '#1' '#2'
  atp spotify playlist tracks https://open.spotify.com/playlist/3cEYpjA9oz9GiPac4AsH4n

More: atp guide spotify playlist";

const EX_CREATE: &str = "\
Examples:
  atp spotify playlist create \"Road trip\"
  atp spotify playlist create Focus --private -d \"Instrumental only\"
  atp spotify playlist create \"Party\" --collaborative

More: atp guide spotify playlist create";

const EX_ADD: &str = "\
Examples:
  atp spotify search \"daft punk\" -n 3
  atp spotify playlist add 3cEYpjA9oz9GiPac4AsH4n '#1' '#3'
  atp spotify playlist add playlist:3cEYpjA9oz9GiPac4AsH4n spotify:track:4uLU6hMCjMI75M1A2tKUQC
  atp spotify playlist add 3cEYpjA9oz9GiPac4AsH4n episode:512ojhOuo1ktJprKbVcKyQ

More: atp guide spotify playlist add";

const EX_REMOVE: &str = "\
Examples:
  atp spotify playlist remove 3cEYpjA9oz9GiPac4AsH4n 4uLU6hMCjMI75M1A2tKUQC
  atp spotify playlist tracks 3cEYpjA9oz9GiPac4AsH4n && atp spotify playlist remove 3cEYpjA9oz9GiPac4AsH4n '#4'

More: atp guide spotify playlist remove";

const EX_DELETE: &str = "\
Examples:
  atp spotify playlist delete 3cEYpjA9oz9GiPac4AsH4n
  atp spotify playlist delete https://open.spotify.com/playlist/3cEYpjA9oz9GiPac4AsH4n --yes

More: atp guide spotify playlist delete";

const EX_TRACKS: &str = "\
Examples:
  atp spotify user-posts                  # your playlists
  atp spotify playlist tracks '#1' -n 200
  atp spotify playlist tracks spotify:playlist:3cEYpjA9oz9GiPac4AsH4n -f csv > list.csv

More: atp guide spotify playlist tracks";

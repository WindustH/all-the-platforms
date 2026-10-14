//! Spotify-only commands: your playlists (create, add, remove, delete, the
//! items of one) and playback control.

mod player;
mod playlist;

use atp_core::{Data, Result};

use crate::api::Api;

pub use self::player::PlayerCommand;
pub use self::playlist::PlaylistCommand;

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Your playlists: create, add or remove items, delete; the items of one
  #[command(subcommand, after_long_help = playlist::EX)]
  Playlist(PlaylistCommand),
  /// Playback on your devices: status, play, pause, skip, seek, volume,
  /// queue, transfer (needs Premium)
  #[command(subcommand, after_long_help = player::EX)]
  Player(PlayerCommand),
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  match command {
    Command::Playlist(command) => playlist::run(api, command).await,
    Command::Player(command) => player::run(api, command).await,
  }
}

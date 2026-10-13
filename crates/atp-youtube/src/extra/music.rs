//! YouTube Music: search, the songs of an album, artist or playlist, lyrics.

use atp_core::{Data, Page, Result};

use crate::api::Api;
use crate::music::{self, Found};
use crate::refs;

#[derive(Debug, clap::Subcommand)]
pub enum MusicCommand {
  /// Search YouTube Music
  Search {
    query: String,
    /// What to look for
    #[arg(short = 't', long = "type", default_value = "songs", value_parser = clap::builder::PossibleValuesParser::new(crate::music::KINDS.iter().map(|(k, _)| *k)))]
    kind: String,
  },
  /// Songs of an album (MPREb…), artist (UC…) or playlist
  Tracks { id: String },
  /// Lyrics of a song
  Lyrics { video: String },
}

pub async fn run(api: &Api, command: MusicCommand) -> Result<Data> {
  Ok(match command {
    MusicCommand::Search { query, kind } => match music::search(api, &query, &kind).await? {
      Found::Songs(songs) => Data::Posts(Page::last(songs)),
      Found::Albums(albums) => Data::Collections(Page::last(albums)),
      Found::Artists(artists) => Data::Users(Page::last(artists)),
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

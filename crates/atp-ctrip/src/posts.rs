//! `read`: a sight, a hotel, a note or an archived travel note, by link or
//! prefixed id.

use atp_core::{Error, Post, Result};

use crate::api::Api;
use crate::refs::{self, Target};
use crate::{hotels, notes, sights};

pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  match refs::target(arg)? {
    Target::Sight(sight) => sights::read(api, &sight).await,
    Target::Hotel(id) => hotels::read(api, &id).await,
    Target::Note(id) => notes::read(api, &id).await,
    Target::Travel(url) => notes::travel(api, &url).await,
    Target::Ranking(id) => Err(Error::input(format!(
      "a ranking is a list: run `ranking {id}`"
    ))),
  }
}

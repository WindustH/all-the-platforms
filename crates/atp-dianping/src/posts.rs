//! `read`: a shop from its mobile page (open to visitors by numeric id;
//! phone and address come masked), or a discovery note from its page.

use atp_core::{Error, Post, Result};
use atp_mtsec::guard::Call;
use atp_mtsec::shops::{self, Shop};

use crate::api::{Api, MOBILE};
use crate::parse;
use crate::refs::{self, Target, note_url};

pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  match refs::target(arg)? {
    Target::Shop(shop) => shop_page(api, &shop).await,
    Target::Note(id) => note(api, &id).await,
  }
}

async fn shop_page(api: &Api, shop: &Shop) -> Result<Post> {
  let key = match shop {
    Shop::Id(key) | Shop::Uuid(key) => key,
  };
  let html = shops::page(&api.ctx, key).await?;
  let cache = parse::assigned(&html, "window.__xhrCache__")
    .ok_or_else(|| Error::upstream(format!("no shop data in Dianping's page of {key}")))?;
  let post =
    parse::shop(&cache).ok_or_else(|| Error::not_found(format!("Dianping shows no shop {key}")))?;
  if let Some(uuid) = post.extra.get("shop_uuid").and_then(|u| u.as_str()) {
    shops::remember(&api.ctx, &post.id, uuid);
  }
  Ok(post)
}

async fn note(api: &Api, id: &str) -> Result<Post> {
  let call = Call::page(note_url(id), format!("{MOBILE}/")).mobile();
  let html = api.html(call).await?;
  parse::assigned(&html, "window.__xhrCache__")
    .and_then(|cache| parse::note(&cache, id))
    .ok_or_else(|| Error::not_found(format!("no Dianping note {id}")))
}

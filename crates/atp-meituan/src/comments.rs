//! `comments SHOP`: reviews of a 到店 shop. Meituan's pages show them from
//! Dianping's backend, so atp reads Dianping's review list
//! ([`atp_mtsec::reviews`]) for the shop as Dianping names it, with the
//! session's Dianping login.

use atp_core::{Comment, Error, Page, PageReq, Result};
use atp_mtsec::{reviews, shops};

use crate::api::Api;

pub async fn list(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Comment>> {
  let shop = shops::parse(arg).ok_or_else(|| {
    Error::input(format!(
      "not a Dianping shop: {arg} (Meituan's 到店 reviews are Dianping's: give the shop's \
       Dianping id, uuid or dianping.com link)"
    ))
  })?;
  if !api.has_dianping() {
    return Err(
      Error::auth("reviews come from Dianping and need a Dianping login (cookie dper)")
        .with_hint("log in on www.dianping.com in your browser, then run `atp mt login --browser`"),
    );
  }
  let uuid = shops::uuid(&api.ctx, &shop).await?;
  reviews::list(&api.ctx, &uuid, page).await
}

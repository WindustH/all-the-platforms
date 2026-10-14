//! `comments SHOP`: the shop's reviews from Dianping's review list (see
//! [`atp_mtsec::reviews`]); a numeric shop id is turned into the uuid the
//! list wants through the shop's mobile page, once.

use atp_core::{Comment, Page, PageReq, Result};
use atp_mtsec::{reviews, shops};

use crate::api::Api;
use crate::refs;

pub async fn list(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Comment>> {
  let shop = refs::shop(arg)?;
  api.require_login()?;
  let uuid = shops::uuid(&api.ctx, &shop).await?;
  reviews::list(&api.ctx, &uuid, page).await
}

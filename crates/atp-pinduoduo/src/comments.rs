//! Reviews of a goods (`/reviews/<goods>/list`, the path the app uses with
//! `enable_order_anonym=1`) and their summary (`/reviews/<goods>/info`):
//! both need a login and `anti_content`. PDD shows only part of the
//! reviews of goods with many.

use atp_core::{Comment, Error, Page, PageReq, Result, Value, ValueExt};

use crate::api::{Api, goods_url};
use crate::parse;
use crate::refs::{self, Goods};

const PAGE_SIZE: usize = 20;

/// The goods id behind a reference; reviews have no DDK route.
pub async fn goods_id(api: &Api, post: &str) -> Result<String> {
  match refs::goods(api, post).await? {
    Goods::Id(id) => Ok(id),
    Goods::Sign(_) => Err(Error::input(
      "reviews need a goods id or goods.html link, not a 多多进宝 goods_sign",
    )),
  }
}

/// Cursors are page numbers.
pub async fn list(api: &Api, post: &str, page: &PageReq) -> Result<Page<Comment>> {
  api.require_session()?;
  let id = goods_id(api, post).await?;
  let number = page.number_or(1);
  let v = list_page(api, &id, number).await?;
  let items: Vec<Comment> = v.list("data").iter().filter_map(parse::review).collect();
  let more = items.len() >= PAGE_SIZE && v.bool("has_more") != Some(false);
  Ok(Page::new(items, more.then(|| (number + 1).to_string())))
}

/// One page of `/reviews/<goods>/list`.
pub async fn list_page(api: &Api, goods: &str, number: u64) -> Result<Value> {
  api
    .get(
      &format!("/reviews/{goods}/list"),
      &[
        ("page", number.to_string()),
        ("size", PAGE_SIZE.to_string()),
        ("enable_video", "1".into()),
        ("enable_group_review", "1".into()),
        ("label_id", "0".into()),
        ("enable_order_anonym", "1".into()),
      ],
      &goods_url(goods),
    )
    .await
}

/// The review summary: rating, label tags with counts, flags.
pub async fn summary(api: &Api, post: &str) -> Result<Value> {
  api.require_session()?;
  let id = goods_id(api, post).await?;
  api
    .get(
      &format!("/reviews/{id}/info"),
      &[
        ("enable_video", "1".into()),
        ("enable_group_review", "1".into()),
        ("enable_merge_outer", "1".into()),
      ],
      &goods_url(&id),
    )
    .await
}

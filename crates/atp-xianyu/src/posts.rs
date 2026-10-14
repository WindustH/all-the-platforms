//! Items: one item's page (`read`, `download`) and the home feed.

use atp_core::{Error, Page, PageReq, Post, Result, ValueExt, json};

use crate::api::{self, Api};
use crate::{parse, refs};

const FEED_PAGE: u64 = 30;

pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let id = refs::item(api, arg).await?;
  let v = api
    .call(
      "mtop.taobao.idle.pc.detail",
      json!({"itemId": id}),
      api::ITEM,
    )
    .await?;
  parse::item(&v).ok_or_else(|| Error::not_found(format!("Xianyu has no item {id}")))
}

/// The home feed of recommended items.
pub async fn feed(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  let number = page.number_or(1);
  let data = json!({"itemId": "", "pageSize": FEED_PAGE, "pageNumber": number, "machId": ""});
  let v = api
    .call("mtop.taobao.idlehome.home.webpc.feed", data, api::HOME)
    .await?;
  let items: Vec<Post> = v.list("cardList").iter().filter_map(parse::card).collect();
  let more = !items.is_empty() && v.bool("nextPage") != Some(false);
  Ok(Page::new(items, more.then(|| (number + 1).to_string())))
}

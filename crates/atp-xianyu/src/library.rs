//! The logged-in account's favorites (收藏), newest first.

use atp_core::{Error, Page, PageReq, Post, Result, ValueExt, json};

use crate::api::{self, Api};
use crate::parse;

const PAGE: u64 = 20;

pub async fn favorites(api: &Api, user: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let me = api.me()?;
  if user.is_some_and(|u| u != "me" && u != me) {
    return Err(Error::input(
      "Xianyu shows favorites of your own account only",
    ));
  }
  let number = page.number_or(1);
  let data = json!({"pageNumber": number, "rowsPerPage": PAGE, "type": "DEFAULT"});
  let v = api
    .call("mtop.taobao.idle.web.favor.item.list", data, api::PERSONAL)
    .await?;
  let items: Vec<Post> = v.list("items").iter().filter_map(parse::card).collect();
  let more = !items.is_empty() && v.bool("nextPage") == Some(true);
  Ok(Page::new(items, more.then(|| (number + 1).to_string())))
}

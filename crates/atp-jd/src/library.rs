//! The account's followed items (`favorites`) and browsing history
//! (`history`), read-only, from the item pages' signed calls.

use atp_core::{Page, PageReq, Post, Result, json};

use crate::api::{Api, Call, From};
use crate::parse;
use crate::posts::{ORDER, SIGN_APP};

const PAGE_SIZE: usize = 20;

pub async fn favorites(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  items(api, "pc_follow_product_new", "pc_sx_follow_product", page).await
}

pub async fn history(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  items(api, "pc_myjd_getBrowseHistory", "pc_sx_history", page).await
}

async fn items(
  api: &Api,
  function: &'static str,
  source: &str,
  page: &PageReq,
) -> Result<Page<Post>> {
  api.require_login()?;
  let number = page.number_or(1);
  let size = page.size_within(PAGE_SIZE);
  let body = json!({ "pageNo": number, "pageSize": size, "tag": 1, "source": source });
  let v = api
    .call(Call {
      function,
      appid: "item-v3",
      path: "/",
      sign: Some(SIGN_APP),
      body: Some(body.to_string()),
      params: vec![("area", api.area())],
      from: From {
        page: Some(("https://item.jd.com/".into(), "h5_2.2.0")),
        ..From::item("")
      },
      form: false,
      time: true,
      order: ORDER,
    })
    .await?;
  let items = parse::items(&v);
  let next = (items.len() >= size).then(|| (number + 1).to_string());
  Ok(Page::new(items, next))
}

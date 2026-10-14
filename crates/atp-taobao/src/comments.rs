//! `comments ITEM`: an item's reviews, 20 a page; `--sort` picks all,
//! those with pictures, or good, neutral or bad ones.

use atp_alibaba::mtop::Call;
use atp_core::{Comment, Page, PageReq, Result, ValueExt, json};

use crate::api::{Api, item_url};
use crate::{parse, refs};

pub const SORTS: &[&str] = &["all", "pictures", "good", "neutral", "bad"];
const PAGE: u64 = 20;

pub async fn list(
  api: &Api,
  post: &str,
  sort: Option<&str>,
  page: &PageReq,
) -> Result<Page<Comment>> {
  let id = refs::item(api, post).await?;
  let number = page.number_or(1);
  let rate_type = match sort {
    Some("pictures") => "7",
    Some("good") => "1",
    Some("neutral") => "0",
    Some("bad") => "-1",
    _ => "",
  };
  let data = json!({
    "auctionNumId": id,
    "expression": "",
    "foldFlag": "0",
    "orderType": "",
    "pageNo": number,
    "pageSize": PAGE,
    "rateSrc": "pc_rate_list",
    "rateType": rate_type,
    "searchImpr": "-8",
    "showTrueCount": false,
    "skuVids": "",
  });
  let referer = item_url(&id);
  let call = Call::new("mtop.taobao.rate.detaillist.get", "6.0", &data)
    .query("ecode", "1")
    .header("referer", referer);
  let v = api.mtop(call).await?;
  let items: Vec<Comment> = v
    .list("rateList")
    .iter()
    .filter_map(parse::review)
    .collect();
  let more = !items.is_empty() && v.u64("totalPage").is_some_and(|last| number < last);
  Ok(Page::new(items, more.then(|| (number + 1).to_string())))
}

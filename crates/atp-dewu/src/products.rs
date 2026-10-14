//! Products of the PC site (www.dewu.com → app.dewu.com, signed): the home
//! page's pick tabs and their products (`hot`, `tabs`, `tab`), and a
//! product page (`read`): details, gallery, sizes and per-size buy prices.

use std::collections::BTreeMap;
use std::time::Duration;

use atp_core::{Collection, Error, Page, PageReq, Post, Result, Value, ValueExt, json, paging};

use crate::api::{Api, WWW, product_url};
use crate::parse;

const PICK: &str = "/api/v1/h5/commodity-pick-interfaces/pc/pick-rule-result";
const DETAIL: &str = "/api/v1/h5/pc/index/fire/flow/product/detailV3";
/// Per-size buy prices, as the live product-detail bundle names the call.
const BUY_LAYER: &str = "/api/v1/h5/trade-price-center/price/h5/buy-layer";
/// Headers the product page's module adds to its calls.
const APP_HEADERS: &[(&str, &str)] = &[
  ("appVersion", "5.31.0"),
  ("appId", "h5"),
  ("platform", "h5"),
];
/// Products per page of a pick feed, as the home page asks.
const PAGE_SIZE: u64 = 24;
const TABS_CACHE: &str = "tabs";
const TABS_TTL: Duration = Duration::from_secs(24 * 3600);

fn home() -> String {
  format!("{WWW}/")
}

/// A numeric id as the pages send it (a JSON number), else as text.
fn id_value(id: &str) -> Value {
  id.parse::<u64>()
    .map(Value::from)
    .unwrap_or_else(|_| Value::from(id))
}

/// The home page's tabs, cached for a day.
async fn all_tabs(api: &Api) -> Result<Vec<Collection>> {
  let list = match api.ctx.store.cache_get::<Value>(TABS_CACHE, TABS_TTL) {
    Some(list) => list,
    None => {
      let body = json!({ "pageNum": 1, "pageSize": 20 });
      let path = format!("{PICK}/category-pick/mapping");
      let v = api.post(&path, body, &home(), &[]).await?;
      let list = v.at("data.checkRespDTOList").clone();
      api.ctx.store.cache_put(TABS_CACHE, &list);
      list
    }
  };
  Ok(list.list("").iter().filter_map(parse::tab).collect())
}

pub async fn tabs(api: &Api, page: &PageReq) -> Result<Page<Collection>> {
  Ok(paging::slice(all_tabs(api).await?, page))
}

/// `hot`: the first tab's products; cursors are `<pickId>:<page>`.
pub async fn hot(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  let (pick, number) = match page.cursor.as_deref().and_then(|c| c.split_once(':')) {
    Some((pick, number)) => (pick.to_owned(), number.parse().unwrap_or(1)),
    None => {
      let first = all_tabs(api).await?.into_iter().next();
      let tab = first.ok_or_else(|| Error::upstream("the Dewu home page lists no tabs"))?;
      (tab.id, 1)
    }
  };
  let (items, more) = picks(api, &pick, number).await?;
  Ok(Page::new(
    items,
    more.then(|| format!("{pick}:{}", number + 1)),
  ))
}

/// One tab's products; cursors are page numbers.
pub async fn tab(api: &Api, pick: &str, page: &PageReq) -> Result<Page<Post>> {
  let number = page.number_or(1);
  let (items, more) = picks(api, pick, number).await?;
  Ok(Page::new(items, more.then(|| (number + 1).to_string())))
}

/// A page of a pick feed and whether another may follow.
async fn picks(api: &Api, pick: &str, number: u64) -> Result<(Vec<Post>, bool)> {
  let body = json!({
    "pickRuleId": id_value(pick), "pageNum": number, "pageSize": PAGE_SIZE,
    "filterUnbid": true, "showCspu": true,
  });
  let v = api
    .post(&format!("{PICK}/feeds/info"), body, &home(), &[])
    .await?;
  let items: Vec<Post> = v.list("data.list").iter().filter_map(parse::pick).collect();
  let more = items.len() as u64 >= PAGE_SIZE && v.bool("data.hasMore") != Some(false);
  Ok((items, more))
}

/// `read`: the product page with per-size buy prices; when those are
/// refused, the page's own price stays and `extra.buy_prices_error` says
/// why.
pub async fn read(api: &Api, spu: &str) -> Result<Post> {
  let mut post = detail(api, spu).await?;
  match buy_prices(api, spu).await {
    Ok(prices) => parse::add_buy_prices(&mut post, &prices),
    Err(e) => {
      tracing::warn!("buy-layer of {spu}: {e}");
      post
        .extra
        .insert("buy_prices_error".into(), e.message.into());
    }
  }
  Ok(post)
}

/// The product page (`detailV3`) as the PC page asks for it: no SKU, no
/// property (`NaN` there, sent as null), `sourceName` `pc`.
pub async fn detail(api: &Api, spu: &str) -> Result<Post> {
  let body = json!({
    "spuId": id_value(spu), "propertyValueId": null, "sourceName": "pc",
    "abTests": [{ "name": "newsr", "value": "2" }, { "name": "5.16_certificate", "value": "1" }],
    "extDatas": [{ "name": "CUSTOM_RECOMMEND_SWITCH", "value": "0" }],
  });
  let v = api
    .post(DETAIL, body, &product_url(spu), APP_HEADERS)
    .await?;
  parse::product(v.at("data"), spu)
    .ok_or_else(|| Error::not_found(format!("Dewu has no product {spu}")))
}

async fn buy_prices(api: &Api, spu: &str) -> Result<BTreeMap<String, f64>> {
  let body = json!({ "spuId": id_value(spu), "abTest": true });
  let v = api
    .post(BUY_LAYER, body, &product_url(spu), APP_HEADERS)
    .await?;
  Ok(parse::buy_prices(v.at("data")))
}

//! Official market data through the POIZON Open Platform (credentials
//! needed): an article number's catalogue entry (`search`), per-size
//! reference prices (`prices`) and 30-day sales (`sales`). The catalogue
//! is read in the US region, whose catalogue covers the most products;
//! prices in the region asked for.

use atp_core::{Error, Page, PageReq, Post, Query, Result, Value, json, paging};

use crate::api::Api;
use crate::parse;

const BY_ARTICLE: &str =
  "/dop/api/v1/pop/api/v1/intl-commodity/intl/sku/sku-basic-info/by-article-number";
const BY_GLOBAL_SKU: &str =
  "/dop/api/v1/pop/api/v1/intl-commodity/intl/sku/sku-basic-info/by-global-sku";
const BATCH_PRICE: &str = "/dop/api/v1/pop/api/v1/recommend-bid/batchPrice";
const CATALOGUE_REGION: &str = "US";
/// SKUs per `batchPrice` call (the API's limit).
const PRICE_BATCH: usize = 20;
/// An article number every catalogue has (Nike Dunk Low "Panda"), for
/// checking credentials.
const CHECK_ARTICLE: &str = "DD1391-100";

/// Where and how prices are asked for.
pub struct Market {
  /// Region and country code (`CN`, `US`, `HK` …).
  pub region: String,
  /// ISO currency (`CNY`, `USD` …).
  pub currency: String,
  /// Consignment (寄售) bids instead of spot (现货) ones.
  pub consign: bool,
}

/// `search ARTICLE`: the products filed under an article number (货号).
pub async fn search(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  api.open.require(&api.ctx)?;
  Ok(paging::slice(catalogue(api, &query.keyword).await?, page))
}

/// Check the credentials with one catalogue lookup.
pub async fn check(api: &Api) -> Result<()> {
  lookup(api, CHECK_ARTICLE).await.map(|_| ())
}

/// The catalogue entries of an article number; article numbers are case
/// sensitive, so a lowercase one that finds nothing is tried in capitals.
async fn catalogue(api: &Api, article: &str) -> Result<Vec<Post>> {
  let article = article.trim();
  if article.is_empty() {
    return Err(Error::input(
      "give an article number (货号), e.g. DD1391-100",
    ));
  }
  let mut data = lookup(api, article).await?;
  let upper = article.to_uppercase();
  if data.is_empty() && upper != article {
    data = lookup(api, &upper).await?;
  }
  Ok(data.iter().filter_map(parse::catalogue).collect())
}

async fn lookup(api: &Api, article: &str) -> Result<Vec<Value>> {
  let biz = json!({
    "articleNumber": article, "region": CATALOGUE_REGION, "language": "en",
    "buyStatusEnable": true,
  });
  let v = api.open.call(&api.ctx, BY_ARTICLE, biz).await?;
  Ok(v.as_array().cloned().unwrap_or_default())
}

async fn first(api: &Api, article: &str) -> Result<Post> {
  catalogue(api, article)
    .await?
    .into_iter()
    .next()
    .ok_or_else(|| Error::not_found(format!("the Open Platform has no article {article}")))
}

/// Ids as the API takes them (numbers).
fn ids(skus: &[String]) -> Vec<Value> {
  skus
    .iter()
    .map(|id| {
      id.parse::<u64>()
        .map(Value::from)
        .unwrap_or_else(|_| Value::from(id.as_str()))
    })
    .collect()
}

/// Per-size reference prices (`batchPrice`, 20 SKUs a call).
pub async fn prices(api: &Api, article: &str, market: &Market) -> Result<Post> {
  let mut post = first(api, article).await?;
  let skus = parse::global_skus(&post);
  let mut rows = Vec::new();
  for chunk in skus.chunks(PRICE_BATCH) {
    let biz = json!({
      "globalSkuIdList": ids(chunk), "region": market.region, "currency": market.currency,
      "countryCode": market.region, "biddingType": if market.consign { 25 } else { 20 },
      "saleType": 7,
    });
    let v = api.open.call(&api.ctx, BATCH_PRICE, biz).await?;
    rows.extend(v.as_array().cloned().unwrap_or_default());
  }
  parse::add_prices(&mut post, &rows, &market.currency);
  Ok(post)
}

/// 30-day sales per size (`by-global-sku` with statistics turned on).
pub async fn sales(api: &Api, article: &str) -> Result<Post> {
  let mut post = first(api, article).await?;
  let skus = parse::global_skus(&post);
  let biz = json!({
    "globalSkuIds": ids(&skus), "region": CATALOGUE_REGION, "language": "en",
    "buyStatusEnable": true,
    "statisticsDataQry": { "salesEnable": true, "minPriceEnable": true },
  });
  let v = api.open.call(&api.ctx, BY_GLOBAL_SKU, biz).await?;
  parse::add_sales(&mut post, &v);
  Ok(post)
}

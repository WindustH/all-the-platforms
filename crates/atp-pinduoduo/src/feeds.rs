//! `feed`: the home page's recommendations (the server-rendered first 20,
//! then `alexa/cells/hub/v3` with `anti_content`; visitors get a short
//! "risk" list of a few pages). `hot`: the search box's hot words from the
//! home page, or a 多多进宝 ranking.

use atp_core::{Error, Page, PageReq, Post, Result, ValueExt, paging};

use crate::api::{Api, WEB};
use crate::parse;

/// `hot --category`: hot words, or the DDK lists 今日销量榜 and 实时热销榜.
pub const HOT: &[&str] = &["words", "sales", "realtime"];
const PAGE_SIZE: u64 = 20;

fn home() -> String {
  format!("{WEB}/")
}

/// Cursors are `<offset>:<list_id>`.
pub async fn feed(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  match page.cursor.as_deref().and_then(|c| c.split_once(':')) {
    Some((offset, list)) => more(api, offset.parse().unwrap_or(PAGE_SIZE), list).await,
    None => first(api).await,
  }
}

async fn first(api: &Api) -> Result<Page<Post>> {
  let store = api.store(&home()).await?;
  let items: Vec<Post> = store
    .list("mainListProps.goodsList")
    .iter()
    .filter_map(parse::goods)
    .collect();
  let list = store
    .first_str(&["mainListProps.listID", "listID"])
    .unwrap_or_else(|| "pdd".into());
  let offset = store
    .u64("mainListProps.offset")
    .unwrap_or(items.len() as u64);
  let next = (!items.is_empty()).then(|| format!("{offset}:{list}"));
  Ok(Page::new(items, next))
}

async fn more(api: &Api, offset: u64, list: &str) -> Result<Page<Post>> {
  let v = api
    .get(
      "/api/alexa/cells/hub/v3",
      &[
        ("platform", "H5".into()),
        ("page_sn", "10002".into()),
        ("page_id", "index_list.html".into()),
        ("engine_version", "3.0".into()),
        ("offset", offset.to_string()),
        ("count", PAGE_SIZE.to_string()),
        ("list_id", list.to_owned()),
      ],
      &home(),
    )
    .await?;
  let items: Vec<Post> = v
    .list("data.goods_list")
    .iter()
    .filter_map(parse::goods)
    .collect();
  let more = v.bool("has_more").unwrap_or(true) && !items.is_empty();
  let next = more.then(|| format!("{}:{list}", offset + PAGE_SIZE));
  Ok(Page::new(items, next))
}

pub async fn hot(api: &Api, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  match category.unwrap_or("words") {
    "sales" => ranking(api, 1, page).await,
    "realtime" => ranking(api, 5, page).await,
    _ => words(api, page).await,
  }
}

/// The hot words under the home page's search box.
async fn words(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  let store = api.store(&home()).await?;
  let goods = store.at("searchSSRData.goods");
  let mut items: Vec<Post> = goods
    .list("hotQueries")
    .iter()
    .filter_map(parse::hot_word)
    .collect();
  if items.is_empty() {
    return Err(Error::upstream("the home page carried no hot words"));
  }
  if let Some(shade) = parse::hot_word(goods.at("shadeQuery"))
    && !items.iter().any(|i| i.id == shade.id)
  {
    items.insert(0, shade);
  }
  Ok(paging::slice(items, page))
}

/// A DDK channel of `pdd.ddk.goods.recommend.get`, 20 a call (offsets
/// must be multiples of the limit); cursors are `<offset>:<list_id>`.
async fn ranking(api: &Api, channel: u8, page: &PageReq) -> Result<Page<Post>> {
  let (offset, list) = page
    .cursor
    .as_deref()
    .and_then(|c| c.split_once(':'))
    .map(|(o, l)| (o.parse().unwrap_or(0), l.to_owned()))
    .unwrap_or((0u64, String::new()));
  let limit = PAGE_SIZE;
  let pid = api.ddk.creds(&api.ctx).map(|c| c.pid).unwrap_or_default();
  let v = api
    .ddk
    .call(
      &api.ctx,
      "pdd.ddk.goods.recommend.get",
      &[
        ("channel_type", channel.to_string()),
        ("offset", offset.to_string()),
        ("limit", limit.to_string()),
        ("list_id", list),
        ("pid", pid),
      ],
    )
    .await?;
  let r = v.at("goods_basic_detail_response");
  let items: Vec<Post> = r.list("list").iter().filter_map(parse::ddk_goods).collect();
  let list = r.str("list_id").unwrap_or_default();
  let next = (items.len() as u64 >= limit).then(|| format!("{}:{list}", offset + limit));
  Ok(Page::new(items, next))
}

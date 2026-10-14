//! `comments`: an item's reviews. The first page is the item page's first
//! screen (`getLegoWareDetailComment`); later pages come from the review
//! list (`getCommentListPage`, signed for another app id), which needs the
//! category and the first review's guid from that screen, so cursors are
//! `<page>:<category>:<guid>`. The first screen also carries the review
//! summary the item page shows (count, good rate, tags): `rating`.

use atp_core::output::note;
use atp_core::{Comment, Page, PageReq, Result, Value, ValueExt, json};

use crate::api::{Api, Call, From};
use crate::posts::{ORDER, SIGN_APP};
use crate::{parse, refs};

/// The review list's h5st app id.
const LIST_SIGN_APP: &str = "01a47";
const LIST_ORDER: &[&str] = &[
  "appid",
  "body",
  "client",
  "clientVersion",
  "functionId",
  "h5st",
  "loginType",
  "t",
  "uuid",
];
const PAGE_SIZE: usize = 10;

pub async fn list(api: &Api, post: &str, page: &PageReq) -> Result<Page<Comment>> {
  let sku = refs::sku(api, post).await?;
  if let Some(cursor) = page.cursor.as_deref() {
    return later(api, &sku, cursor).await;
  }
  let v = first_screen(api, &sku).await?;
  if let Some(line) = summary_line(&v) {
    note(&line);
  }
  let list = v.list("commentInfoList");
  let next = list.first().and_then(|first| {
    let guid = first.str("guid")?;
    let category = first.str("maidianInfo")?.split('_').next()?.to_owned();
    Some(format!("2:{category}:{guid}"))
  });
  Ok(Page::new(
    list.iter().filter_map(parse::comment).collect(),
    next,
  ))
}

/// The review summary of an item: how many reviews, the good rate and
/// the review tags with their counts.
pub async fn rating(api: &Api, post: &str) -> Result<Value> {
  let sku = refs::sku(api, post).await?;
  let v = first_screen(api, &sku).await?;
  let tags: Vec<Value> = v
    .list("tagStatisticsinfoList")
    .iter()
    .filter_map(|t| Some(json!({"name": t.str("name")?, "count": t.at("count")})))
    .collect();
  Ok(json!({
    "item": sku,
    "reviews": v.str("allCntStr"),
    "good_rate": v.str("goodRate"),
    "tags": tags,
  }))
}

/// The item page's review screen (`getLegoWareDetailComment`).
async fn first_screen(api: &Api, sku: &str) -> Result<Value> {
  let body = json!({
    "shopType": "0", "sku": sku.parse::<u64>().unwrap_or_default(),
    "commentNum": PAGE_SIZE, "source": "pc",
  });
  api
    .call(Call {
      function: "getLegoWareDetailComment",
      appid: "item-v3",
      path: "/",
      sign: Some(SIGN_APP),
      body: Some(body.to_string()),
      params: vec![("build", "100000".into())],
      from: From::item(sku),
      form: false,
      time: true,
      order: ORDER,
    })
    .await
}

/// `12万+ reviews, 98% good`, as the item page heads its reviews.
fn summary_line(v: &Value) -> Option<String> {
  let count = v.str("allCntStr").filter(|c| c != "0")?;
  Some(match v.str("goodRate") {
    Some(rate) => format!("{count} reviews, {rate} good"),
    None => format!("{count} reviews"),
  })
}

async fn later(api: &Api, sku: &str, cursor: &str) -> Result<Page<Comment>> {
  let mut parts = cursor.splitn(3, ':');
  let (Some(number), Some(category), Some(guid)) = (parts.next(), parts.next(), parts.next())
  else {
    return Err(atp_core::Error::input(format!(
      "bad review cursor: {cursor}"
    )));
  };
  let number: u64 = number.parse().unwrap_or(2);
  let body = json!({
    "requestSource": "pc", "shopComment": 0, "sameComment": 0, "channel": null,
    "extInfo": {
      "isQzc": "0", "spuId": sku, "commentRate": "1", "needTopAlbum": "1", "bbtf": "",
      "userGroupComment": "1",
    },
    "num": PAGE_SIZE.to_string(), "pictureCommentType": "A", "scval": null,
    "shadowMainSku": "0", "shopType": "0", "firstCommentGuid": guid, "sku": sku,
    "category": category, "shieldCurrentComment": "1", "pageSize": PAGE_SIZE.to_string(),
    "isFirstRequest": false, "style": "1", "isCurrentSku": false, "sortType": "5",
    "tagId": "", "tagType": "", "type": "0", "pageNum": number.to_string(),
  });
  let v = api
    .call(Call {
      function: "getCommentListPage",
      appid: "pc-rate-qa",
      path: "/client.action",
      sign: Some(LIST_SIGN_APP),
      body: Some(body.to_string()),
      params: Vec::new(),
      from: From::item(sku),
      form: true,
      time: true,
      order: LIST_ORDER,
    })
    .await?;
  let mut found = Vec::new();
  collect(v.at("result"), &mut found);
  let items: Vec<Comment> = found.iter().filter_map(parse::comment).collect();
  let next = (!items.is_empty()).then(|| format!("{}:{category}:{guid}", number + 1));
  Ok(Page::new(items, next))
}

/// Review objects anywhere in the list's floors (`commentInfo`).
fn collect(v: &Value, out: &mut Vec<Value>) {
  match v {
    Value::Object(map) => {
      if let Some(info) = map.get("commentInfo").filter(|i| i.is_object()) {
        out.push(info.clone());
      }
      map.values().for_each(|v| collect(v, out));
    }
    Value::Array(items) => items.iter().for_each(|v| collect(v, out)),
    _ => {}
  }
}

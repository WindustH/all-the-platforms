//! `search`: the PC search page's item search (an mtop recommend call, app
//! 34385, with the client fields the page sends), 48 items a page, by
//! relevance, sales or price.

use atp_core::{Page, PageReq, Post, Query, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::parse;

pub const SORTS: &[&str] = &["relevance", "sales", "price-asc", "price-desc"];
const PAGE: u64 = 48;

pub async fn search(api: &Api, query: &Query, page: &PageReq) -> Result<Page<Post>> {
  let number = page.number_or(1);
  let sort = match query.sort.as_deref() {
    Some("sales") => "_sale",
    Some("price-asc") => "_price",
    Some("price-desc") => "_rPrice",
    _ => "",
  };
  let mut params = json!({
    "device": "HMA-AL00", "isBeta": "false", "grayHair": "false", "from": "nt_history",
    "brand": "HUAWEI", "info": "wifi", "index": "4", "rainbow": "", "schemaType": "auction",
    "elderHome": "false", "isEnterSrpSearch": "true", "newSearch": "false", "network": "wifi",
    "subtype": "", "hasPreposeFilter": "false", "prepositionVersion": "v2", "client_os": "Android",
    "gpsEnabled": "false", "searchDoorFrom": "srp", "debug_rerankNewOpenCard": "false",
    "homePageVersion": "v7", "searchElderHomeOpen": "false", "search_action": "initiative",
    "sugg": "_4_1", "sversion": "13.6", "style": "list", "ttid": "600000@taobao_pc_10.7.0",
    "needTabs": "true", "areaCode": "CN", "vm": "nw", "countryNum": "156", "m": "pc",
    "page": number.to_string(), "n": PAGE.to_string(), "q": encode(query.keyword.trim()),
    "qSource": "url", "pageSource": "", "tab": "all",
    "myCNA": api.ctx.http.cookie("cna").unwrap_or_default(),
  });
  if !sort.is_empty() {
    params["sort"] = json!(sort);
  }
  let data = json!({"appId": "34385", "params": params.to_string()});
  let v = api
    .call(
      "mtop.relationrecommend.wirelessrecommend.recommend",
      "2.0",
      data,
      "https://s.taobao.com/",
    )
    .await?;
  let items: Vec<Post> = results(&v).iter().filter_map(parse::search_item).collect();
  let total = v
    .first_count(&["mainInfo.totalResults", "totalResults"])
    .unwrap_or(0);
  let more = items.len() as u64 >= PAGE / 2 && (total == 0 || number * PAGE < total);
  Ok(Page::new(items, more.then(|| (number + 1).to_string())))
}

/// The result list, which moves between answers.
fn results(v: &Value) -> &[Value] {
  ["itemsArray", "listItem", "resultList"]
    .iter()
    .map(|k| v.list(k))
    .chain(
      v.as_object()
        .into_iter()
        .flat_map(|m| m.values())
        .map(|inner| inner.list("itemsArray")),
    )
    .find(|list| !list.is_empty())
    .unwrap_or_default()
}

/// The keyword as the page sends it: `encodeURIComponent`.
fn encode(text: &str) -> String {
  text
    .bytes()
    .map(|b| match b {
      b'A'..=b'Z'
      | b'a'..=b'z'
      | b'0'..=b'9'
      | b'-'
      | b'_'
      | b'.'
      | b'!'
      | b'~'
      | b'*'
      | b'\''
      | b'('
      | b')' => char::from(b).to_string(),
      _ => format!("%{b:02X}"),
    })
    .collect()
}

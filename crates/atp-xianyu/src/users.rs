//! Users: the logged-in account, profiles, the items a user sells and the
//! ratings they received. A profile's lists take the `kcUserId` its header
//! names (which may differ from the id in a link), so their cursors carry
//! it along: `<page>:<kcUserId>`.

use atp_alibaba::account;
use atp_core::{Comment, Page, PageReq, Post, Result, User, Value, ValueExt, json};

use crate::api::{self, Api};
use crate::{parse, refs};

const PAGE: u64 = 20;

pub async fn whoami(api: &Api) -> Result<User> {
  let me = api.me()?;
  let v = api
    .call("mtop.idle.web.user.page.nav", json!({}), api::HOME)
    .await?;
  Ok(parse::me(&v, &me, account::nick(&api.ctx)))
}

async fn head(api: &Api, id: &str) -> Result<Value> {
  let data = json!({"self": api.me().ok().as_deref() == Some(id), "userId": id});
  api
    .call("mtop.idle.web.user.page.head", data, api::PERSONAL)
    .await
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let id = refs::user(api, arg).await?;
  Ok(parse::user(&head(api, &id).await?, &id))
}

/// The id a profile's lists take, and the page number.
async fn listing(api: &Api, arg: &str, page: &PageReq) -> Result<(String, u64)> {
  if let Some((number, kc)) = page.cursor.as_deref().and_then(|c| c.split_once(':')) {
    return Ok((kc.to_owned(), number.parse().unwrap_or(1)));
  }
  let id = refs::user(api, arg).await?;
  let kc = head(api, &id).await?.str("baseInfo.kcUserId").unwrap_or(id);
  Ok((kc, 1))
}

fn next(more: bool, number: u64, kc: &str) -> Option<String> {
  more.then(|| format!("{}:{kc}", number + 1))
}

/// The items a user sells (their pinned item first).
pub async fn posts(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let (kc, number) = listing(api, arg, page).await?;
  let data = json!({"needGroupInfo": true, "pageNumber": number, "userId": kc, "pageSize": PAGE});
  let v = api
    .call("mtop.idle.web.xyh.item.list", data, api::PERSONAL)
    .await?;
  let mut items = Vec::new();
  if number == 1 {
    items.extend(parse::card(v.at("topItem")));
  }
  for card in v.list("cardList") {
    if let Some(post) = parse::card(card).filter(|p| items.iter().all(|i: &Post| i.id != p.id)) {
      items.push(post);
    }
  }
  let more = !items.is_empty() && v.bool("nextPage") == Some(true);
  Ok(Page::new(items, next(more, number, &kc)))
}

/// Ratings the user received, newest first.
pub async fn ratings(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Comment>> {
  let (kc, number) = listing(api, arg, page).await?;
  let data = json!({
    "rateType": 0,
    "ratedUid": kc,
    "raterType": 0,
    "rowsPerPage": PAGE,
    "pageNumber": number,
    "foldFlag": 0,
    "fishAdCode": "330110",
    "extraTag": "",
  });
  let v = api
    .call("mtop.idle.web.trade.rate.list", data, api::PERSONAL)
    .await?;
  let items: Vec<Comment> = v
    .list("cardList")
    .iter()
    .filter_map(parse::rating)
    .collect();
  let more = !items.is_empty() && v.bool("nextPage") == Some(true);
  Ok(Page::new(items, next(more, number, &kc)))
}

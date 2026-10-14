//! `comments TARGET`: reviews of a sight (`13444/json/getCommentCollapseList`,
//! with sub-scores; Ctrip lists at most about 3000 per order), reviews of a
//! hotel (`33278/getHotelCommentList`) or the comments on a note.

use atp_core::{Comment, Error, Page, PageReq, Result, ValueExt, json};

use crate::api::{Api, Site};
use crate::refs::{self, Sight, Target};
use crate::{notes, parse, sights};

/// `comments --sort`: Ctrip's order (`default`), newest first, best rated
/// first, or only the bad ones.
pub const SORTS: &[&str] = &["default", "new", "good", "bad"];
/// Reviews a page holds.
const STEP: u64 = 10;
/// How deep Ctrip lets one order of a sight's reviews go.
const SIGHT_CAP: u64 = 3000;

pub async fn list(
  api: &Api,
  arg: &str,
  sort: Option<&str>,
  page: &PageReq,
) -> Result<Page<Comment>> {
  match refs::target(arg)? {
    Target::Sight(sight) => sight_reviews(api, &sight, sort, page).await,
    Target::Hotel(id) => hotel_reviews(api, &id, sort, page).await,
    Target::Note(id) => notes::comments(api, &id, page).await,
    Target::Travel(_) => Err(Error::unsupported("comments on archived travel notes")),
    Target::Ranking(_) => Err(Error::input("a ranking has no comments; run `ranking ID`")),
  }
}

async fn sight_reviews(
  api: &Api,
  sight: &Sight,
  sort: Option<&str>,
  page: &PageReq,
) -> Result<Page<Comment>> {
  let poi = sights::poi_id(api, sight).await?;
  let number = page.number_or(1);
  let (order, tag) = match sort.unwrap_or("default") {
    "new" => (6, 0),
    "good" => (2, 0),
    "bad" => (1, -12),
    _ => (1, 0),
  };
  let body = json!({
    "arg": {
      "channelType": 2, "collapseType": 0, "commentTagId": tag, "pageIndex": number,
      "pageSize": STEP, "poiId": refs::number(&poi)?, "sourceType": 1,
      "sortType": order, "starType": 0
    }
  });
  let v = api
    .soa("13444/json/getCommentCollapseList", Site::Mobile, body)
    .await?;
  let items: Vec<Comment> = v
    .list("result.items")
    .iter()
    .filter_map(parse::sight_review)
    .collect();
  let total = v
    .u64("result.totalCount")
    .unwrap_or_default()
    .min(SIGHT_CAP);
  let more = !items.is_empty() && number * STEP < total;
  Ok(Page::new(items, more.then(|| (number + 1).to_string())))
}

async fn hotel_reviews(
  api: &Api,
  id: &str,
  sort: Option<&str>,
  page: &PageReq,
) -> Result<Page<Comment>> {
  let number = page.number_or(1);
  // Filters of the review tabs: 1 all, 2 recommended, 3 bad.
  let (order, filter) = match sort.unwrap_or("default") {
    "new" => (1, 1),
    "good" => (0, 2),
    "bad" => (1, 3),
    _ => (0, 1),
  };
  let body = json!({
    "hotelId": refs::number(id)?, "pageIndex": number, "pageSize": STEP,
    "repeatComment": 1, "needStaticInfo": false,
    "functionOptions": ["integratedTopComment", "ctripIntegratedExpediaTaList"],
    "filterInfo": [{ "id": filter, "filterType": 1 }],
    "orderBy": order,
    "head": api.hotel_head("102003")
  });
  let v = api
    .soa("33278/getHotelCommentList", Site::Mobile, body)
    .await?;
  let data = v.at("data");
  let items: Vec<Comment> = data
    .list("commentList")
    .iter()
    .filter_map(parse::hotel_review)
    .collect();
  let total = data
    .first_count(&["totalCountForPage", "totalCount"])
    .unwrap_or_default();
  let more = !items.is_empty() && number * STEP < total;
  Ok(Page::new(items, more.then(|| (number + 1).to_string())))
}

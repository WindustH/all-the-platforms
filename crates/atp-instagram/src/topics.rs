//! Post grids of a hashtag or a place (top / recent). Their cursor is
//! `max_id|page`.

use atp_core::{Page, PageReq, Post, Result, Value, ValueExt};

use crate::api::Api;
use crate::parse;
use crate::refs;

/// Posts of a hashtag (`top` or `recent`).
pub async fn tag(api: &Api, tag: &str, recent: bool, req: &PageReq) -> Result<Page<Post>> {
  let tag = refs::tag(tag);
  let tab = if recent { "recent" } else { "top" };
  let Some(cursor) = &req.cursor else {
    let v = api
      .get("tags/web_info/", &[("tag_name", tag.clone())])
      .await?;
    return Ok(parse::sections(v.at(&format!("data.{tab}"))));
  };
  let (max_id, page) = split(cursor);
  let form = vec![
    ("tab", tab.to_owned()),
    ("max_id", max_id.to_owned()),
    ("page", page.to_owned()),
    ("include_persistent", "0".to_owned()),
    ("surface", "grid".to_owned()),
  ];
  let v = api.post(&format!("tags/{tag}/sections/"), &form).await?;
  Ok(parse::sections(&v))
}

/// Posts of a place (`ranked` or `recent`).
pub async fn location(
  api: &Api,
  location: &str,
  recent: bool,
  req: &PageReq,
) -> Result<Page<Post>> {
  let id = refs::location(location);
  let tab = if recent { "recent" } else { "ranked" };
  let Some(cursor) = &req.cursor else {
    let v = api
      .get("locations/web_info/", &[("location_id", id.clone())])
      .await?;
    return Ok(parse::sections(v.at("native_location_data").at(tab)));
  };
  let (max_id, page) = split(cursor);
  let form = vec![
    ("tab", tab.to_owned()),
    ("max_id", max_id.to_owned()),
    ("page", page.to_owned()),
    ("surface", "grid".to_owned()),
  ];
  let v: Value = api
    .post(&format!("locations/{id}/sections/"), &form)
    .await?;
  Ok(parse::sections(&v))
}

/// `max_id|page` (page 1 when missing).
fn split(cursor: &str) -> (&str, &str) {
  cursor.split_once('|').unwrap_or((cursor, "1"))
}

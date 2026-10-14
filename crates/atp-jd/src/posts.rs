//! `read` and `download`: an item page's data (`pc_detailpage_wareBusiness`,
//! h5st-signed, JD's best-guarded call) with price, shop, stock and images.

use atp_core::{Error, Post, Result};

use crate::api::{Api, Call, From};
use crate::{parse, refs};

/// The item pages' h5st app id.
pub const SIGN_APP: &str = "fb5df";
/// Parameter order of item-page calls.
pub const ORDER: &[&str] = &[
  "functionId",
  "body",
  "h5st",
  "uuid",
  "loginType",
  "appid",
  "clientVersion",
  "client",
  "t",
  "x-api-eid-token",
  "scval",
];

pub async fn read(api: &Api, post: &str) -> Result<Post> {
  let sku = refs::sku(api, post).await?;
  let body = atp_core::json!({
    "skuId": sku, "area": api.area(), "num": "1", "clientSource": "PC",
    "userAgent": "Windows", "sfTime": "1,0,0",
  });
  let v = api
    .call(Call {
      function: "pc_detailpage_wareBusiness",
      appid: "pc-item-soa",
      path: "/",
      sign: Some(SIGN_APP),
      body: Some(body.to_string()),
      params: vec![("scval", sku.clone())],
      from: From::item(&sku),
      form: false,
      time: true,
      order: ORDER,
    })
    .await?;
  parse::item(&v, &sku).ok_or_else(|| Error::not_found(format!("JD has no item {sku}")))
}

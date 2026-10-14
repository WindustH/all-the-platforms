//! Items: one item's details (`read`, `download`).

use atp_core::{Error, Post, Result, json};

use crate::api::{Api, item_url};
use crate::{parse, refs};

pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let id = refs::item(api, arg).await?;
  let ex = json!({
    "id": id,
    "queryParams": format!("id={id}"),
    "domain": "https://item.taobao.com",
    "path_name": "/item.htm",
  });
  let data = json!({"id": id, "detail_v": "3.3.2", "exParams": ex.to_string()});
  let v = api
    .call("mtop.taobao.pcdetail.data.get", "1.0", data, &item_url(&id))
    .await?;
  parse::item(&v, &id).ok_or_else(|| Error::not_found(format!("Taobao has no item {id}")))
}

//! Articles and pins: one by id, a user's, and liking them (点赞).

use atp_core::{Action, Page, PageReq, Post, Result, Value, ValueExt, json};

use crate::api::{AID, Api};
use crate::parse;
use crate::refs::{self, Item};

/// An article with its whole text, or a pin.
pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  match refs::item(arg)? {
    Item::Article(id) => {
      let body = json!({
        "article_id": id,
        "forbid_count": true,
        "is_pre_load": true,
        "req_from": 0,
        "client_type": AID,
      });
      let v = api.post("/content_api/v1/article/detail", &body).await?;
      Ok(parse::article(v.at("data")))
    }
    Item::Pin(id) => {
      let v = api
        .post("/content_api/v1/short_msg/detail", &json!({"msg_id": id}))
        .await?;
      Ok(parse::pin(v.at("data")))
    }
  }
}

/// The next cursor of a `{data, cursor, has_more}` listing.
pub fn next(v: &Value) -> Option<String> {
  let more = v.bool("has_more").unwrap_or(false) && !v.list("data").is_empty();
  more.then(|| v.str("cursor")).flatten()
}

/// A user's articles, the newest first.
pub async fn articles(api: &Api, user: &str, page: &PageReq) -> Result<Page<Post>> {
  let id = refs::user(user)?;
  let body = json!({
    "user_id": id,
    "sort_type": 2,
    "cursor": page.cursor.clone().unwrap_or_else(|| "0".into()),
  });
  let v = api
    .post("/content_api/v1/article/query_list", &body)
    .await?;
  let items = v.list("data").iter().map(parse::article).collect();
  Ok(Page::new(items, next(&v)))
}

/// A user's pins, the newest first.
pub async fn pins(api: &Api, user: &str, page: &PageReq) -> Result<Page<Post>> {
  let id = refs::user(user)?;
  let body = json!({
    "user_id": id,
    "sort_type": 4,
    "cursor": page.cursor.clone().unwrap_or_else(|| "0".into()),
    "limit": page.size_within(20),
  });
  let v = api
    .post("/content_api/v1/short_msg/query_list", &body)
    .await?;
  let items = v.list("data").iter().map(parse::pin).collect();
  Ok(Page::new(items, next(&v)))
}

/// 点赞 an article or pin, or take it back.
pub async fn like(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let item = refs::item(arg)?;
  let body = json!({
    "item_id": item.id(),
    "item_type": item.item_type(),
    "client_type": AID,
  });
  let (path, action) = if undo {
    ("/interact_api/v1/digg/cancel", "unlike")
  } else {
    ("/interact_api/v1/digg/save", "like")
  };
  api.write(path, &body).await?;
  Ok(Action::done(action, arg))
}

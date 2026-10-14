//! Collection sets (收藏集) and collecting articles into them.

use atp_core::{Action, Collection, Error, Page, PageReq, Result, ValueExt, json};

use crate::api::Api;
use crate::parse;
use crate::posts::next;
use crate::refs::{self, Item};
use crate::users;

/// A user's collection sets (yours when USER is omitted).
pub async fn collections(
  api: &Api,
  user: Option<&str>,
  page: &PageReq,
) -> Result<Page<Collection>> {
  let id = match user {
    Some(user) => refs::user(user)?,
    None => users::my_id(api).await?,
  };
  let params = [
    ("user_id", id),
    ("cursor", page.cursor.clone().unwrap_or_else(|| "0".into())),
    ("limit", page.size_within(20).to_string()),
  ];
  let v = api
    .get("/interact_api/v1/collectionSet/list", &params)
    .await?;
  let items = v.list("data").iter().map(parse::collection).collect();
  Ok(Page::new(items, next(&v)))
}

/// Collect an article into a collection set (your first one, the default
/// 收藏集, unless `folder` names another), or take it out.
pub async fn favorite(api: &Api, arg: &str, folder: Option<&str>, undo: bool) -> Result<Action> {
  let Item::Article(article) = refs::item(arg)? else {
    return Err(Error::input("only articles go into collection sets"));
  };
  api.require_login()?;
  let folder = match folder {
    Some(folder) => folder.to_owned(),
    None => first_set(api).await?,
  };
  let (select, unselect) = if undo {
    (vec![], vec![folder.clone()])
  } else {
    (vec![folder.clone()], vec![])
  };
  let body = json!({
    "article_id": article,
    "select_collection_ids": select,
    "unselect_collection_ids": unselect,
    "is_collect_fast": false,
  });
  api
    .write("/interact_api/v2/collectionset/add_article", &body)
    .await?;
  let action = if undo { "unfavorite" } else { "favorite" };
  Ok(Action::done(action, arg).with_message(format!("collection set {folder}")))
}

async fn first_set(api: &Api) -> Result<String> {
  let page = collections(
    api,
    None,
    &PageReq {
      cursor: None,
      size: 1,
    },
  )
  .await?;
  page.items.into_iter().next().map(|c| c.id).ok_or_else(|| {
    Error::input("you have no collection set yet; create one on juejin.cn or give --folder")
  })
}

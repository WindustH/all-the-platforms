//! Collection sets (收藏集): listing them, collecting articles into them,
//! taking them out and moving them between sets, and following other
//! people's sets.

use atp_core::bulk;
use atp_core::{Action, Collection, Error, Page, PageReq, Result, ValueExt, json};

use crate::api::{Api, SITE};
use crate::parse;
use crate::posts::next;
use crate::refs::{self, Item};
use crate::users;

/// The `type` of a collection set in `follow/undo`.
const SET_TYPE: u64 = 10;

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
  let article = article(arg)?;
  api.require_login()?;
  let folder = match folder {
    Some(folder) => refs::collection(folder)?,
    None => first_set(api).await?,
  };
  let (select, unselect) = if undo {
    (None, Some(folder.as_str()))
  } else {
    (Some(folder.as_str()), None)
  };
  collect(api, &article, select, unselect).await?;
  let action = if undo { "unfavorite" } else { "favorite" };
  Ok(Action::done(action, arg).with_message(format!("collection set {folder}")))
}

fn article(arg: &str) -> Result<String> {
  match refs::item(arg)? {
    Item::Article(id) => Ok(id),
    Item::Pin(_) => Err(Error::input("only articles go into collection sets")),
  }
}

/// Put an article into the `select` set and take it out of the
/// `unselect` one in one call, as the collect dialog does.
async fn collect(
  api: &Api,
  article: &str,
  select: Option<&str>,
  unselect: Option<&str>,
) -> Result<()> {
  let body = json!({
    "article_id": article,
    "select_collection_ids": select.into_iter().collect::<Vec<_>>(),
    "unselect_collection_ids": unselect.into_iter().collect::<Vec<_>>(),
    "is_collect_fast": false,
  });
  api
    .write("/interact_api/v2/collectionset/add_article", &body)
    .await?;
  Ok(())
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

/// Every post argument as an article id; the first that is not one stops
/// the command before anything changes.
fn articles(posts: &[String]) -> Result<Vec<String>> {
  posts.iter().map(|p| article(p)).collect()
}

fn post_url(article: &str) -> String {
  format!("{SITE}/post/{article}")
}

/// Collect articles into a set (`add`) or take them out, one call each.
pub async fn file(api: &Api, folder: &str, posts: &[String], add: bool) -> Result<Vec<Action>> {
  let set = refs::collection(folder)?;
  let ids = articles(posts)?;
  let (action, select, unselect) = if add {
    ("folder-add", Some(set.as_str()), None)
  } else {
    ("folder-remove", None, Some(set.as_str()))
  };
  bulk::each(action, &ids, async |article| {
    collect(api, article, select, unselect).await?;
    let done = Action::done(action, article).with_url(post_url(article));
    Ok(done.with_message(format!("collection set {set}")))
  })
  .await
}

/// Move articles from one set to another in one call each, or with
/// `copy` collect them into the second and keep them in the first.
pub async fn shift(
  api: &Api,
  from: &str,
  to: &str,
  posts: &[String],
  copy: bool,
) -> Result<Vec<Action>> {
  let (from, to) = (refs::collection(from)?, refs::collection(to)?);
  let ids = articles(posts)?;
  let (action, unselect) = if copy {
    ("folder-copy", None)
  } else {
    ("folder-move", Some(from.as_str()))
  };
  bulk::each(action, &ids, async |article| {
    collect(api, article, Some(&to), unselect).await?;
    let done = Action::done(action, article).with_url(post_url(article));
    Ok(done.with_message(format!("{from} → {to}")))
  })
  .await
}

/// Follow someone's collection set (`follow/collection`), or unfollow it
/// (`follow/undo`, type 10), as the set's page does.
pub async fn follow(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let id = refs::collection(arg)?;
  if undo {
    api
      .write(
        "/interact_api/v1/follow/undo",
        &json!({"id": id, "type": SET_TYPE}),
      )
      .await?;
  } else {
    api
      .write(
        "/interact_api/v2/follow/collection",
        &json!({"collection_id": id}),
      )
      .await?;
  }
  let action = if undo { "unfollow" } else { "follow" };
  Ok(Action::done(action, &id).with_url(format!("{SITE}/collection/{id}")))
}

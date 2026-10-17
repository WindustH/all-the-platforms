//! Curation lists as follow groups: an `app.bsky.graph.list` record of
//! purpose `curatelist` in your repository, and an
//! `app.bsky.graph.listitem` record (`subject`, `list`) for each account on
//! it. `getLists` and `getList` read them; renaming puts the list record
//! back with its new name, deleting removes the entries with it
//! (`applyWrites`).

use atp_core::bulk;
use atp_core::{
  Action, Collection, Error, ErrorCode, Page, PageReq, Result, User, Value, ValueExt, json,
};

use crate::api::{self, Api, HANDLE_KEY};
use crate::refs::{self, AtUri, LIST};
use crate::{identity, parse, users};

const LIST_ITEM: &str = "app.bsky.graph.listitem";
const CURATE: &str = "app.bsky.graph.defs#curatelist";
/// The longest list name (bytes).
const NAME_MAX: usize = 64;
/// Records deleted per `applyWrites` call.
const WRITES: usize = 10;
/// Pages of entries read at most (100 each).
const PAGES: usize = 100;

/// Your curation lists.
pub async fn groups(api: &Api, page: &PageReq) -> Result<Page<Collection>> {
  let query = vec![("actor", api.did()?), ("purposes", "curatelist".to_owned())];
  let mut lists = api
    .list("app.bsky.graph.getLists", query, "lists", page, |v| {
      Some(parse::list(v))
    })
    .await?;
  lists
    .items
    .retain(|l| l.extra.get("purpose") == Some(&Value::from("curatelist")));
  Ok(lists)
}

/// A list argument with its repository as a DID.
async fn list_ref(api: &Api, arg: &str) -> Result<AtUri> {
  let mut at = refs::list(arg)?;
  at.repo = identity::did(api, &at.repo).await?;
  Ok(at)
}

/// One of your lists.
async fn own(api: &Api, arg: &str) -> Result<AtUri> {
  let did = api.did()?;
  let at = list_ref(api, arg).await?;
  if at.repo != did {
    return Err(Error::new(
      ErrorCode::PermissionDenied,
      format!("{} is not your list", at.uri()),
    ));
  }
  Ok(at)
}

/// The accounts on a list, each with its entry's record in
/// `extra.list_item`.
pub async fn members(api: &Api, arg: &str, page: &PageReq) -> Result<Page<User>> {
  let uri = list_ref(api, arg).await?.uri();
  api
    .list(
      "app.bsky.graph.getList",
      vec![("list", uri)],
      "items",
      page,
      |v| {
        let mut user = parse::user(v.at("subject"));
        if let Some(item) = v.str("uri") {
          user.extra.insert("list_item".into(), item.into());
        }
        Some(user)
      },
    )
    .await
}

/// `(DID, entry record URI)` of everyone on a list.
async fn entries(api: &Api, list: &str) -> Result<Vec<(String, String)>> {
  let mut out = Vec::new();
  let mut cursor: Option<String> = None;
  for _ in 0..PAGES {
    let mut query = vec![("list", list.to_owned()), ("limit", "100".to_owned())];
    if let Some(c) = &cursor {
      query.push(("cursor", c.clone()));
    }
    let v = api.get_auth("app.bsky.graph.getList", query).await?;
    let items = v.list("items");
    out.extend(
      items
        .iter()
        .filter_map(|i| Some((i.str("subject.did")?, i.str("uri")?))),
    );
    cursor = v.str("cursor").filter(|_| !items.is_empty());
    if cursor.is_none() {
      break;
    }
  }
  Ok(out)
}

/// A new curation list.
pub async fn create(api: &Api, name: &str) -> Result<Action> {
  check_name(name)?;
  let record = json!({"$type": LIST, "purpose": CURATE, "name": name, "createdAt": api::now()});
  let v = api.create_record(LIST, record).await?;
  let uri = v.str("uri").unwrap_or_default();
  let mut action = Action::done("follow-group-create", name).with_id(&uri);
  if let Some(at) = AtUri::parse(&uri) {
    let owner = api
      .ctx
      .extra(HANDLE_KEY)
      .filter(|h| !h.is_empty())
      .unwrap_or(at.repo);
    action = action.with_url(refs::list_url(&owner, &at.rkey));
  }
  Ok(action)
}

fn check_name(name: &str) -> Result<()> {
  if name.len() > NAME_MAX {
    return Err(Error::input(format!(
      "a list name takes {NAME_MAX} bytes at most"
    )));
  }
  Ok(())
}

/// Give a list a new name: its record is read and put back changed.
pub async fn rename(api: &Api, arg: &str, name: &str) -> Result<Action> {
  check_name(name)?;
  let at = own(api, arg).await?;
  let query = vec![
    ("repo", at.repo.clone()),
    ("collection", LIST.to_owned()),
    ("rkey", at.rkey.clone()),
  ];
  let v = api.get_auth("com.atproto.repo.getRecord", query).await?;
  let mut record = v.at("value").clone();
  if !record.is_object() {
    return Err(Error::not_found(format!("no list record {}", at.uri())));
  }
  record["name"] = name.into();
  let mut body = json!({"repo": at.repo, "collection": LIST, "rkey": at.rkey, "record": record});
  if let Some(cid) = v.str("cid") {
    body["swapRecord"] = cid.into();
  }
  api.call("com.atproto.repo.putRecord", body).await?;
  Ok(Action::done("follow-group-rename", at.uri()).with_message(name.to_owned()))
}

/// Delete a list with its entries, ten records per `applyWrites`.
pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  let at = own(api, arg).await?;
  let entries = entries(api, &at.uri()).await?;
  let mut records: Vec<AtUri> = entries
    .iter()
    .filter_map(|(_, uri)| AtUri::parse(uri))
    .filter(|r| r.repo == at.repo)
    .collect();
  records.push(at.clone());
  for chunk in records.chunks(WRITES) {
    let writes: Vec<Value> = chunk
      .iter()
      .map(|r| {
        json!({
          "$type": "com.atproto.repo.applyWrites#delete",
          "collection": r.collection,
          "rkey": r.rkey,
        })
      })
      .collect();
    let body = json!({"repo": at.repo, "writes": writes});
    api.call("com.atproto.repo.applyWrites", body).await?;
  }
  let message = format!("{} entries removed with it", records.len() - 1);
  Ok(Action::done("follow-group-delete", at.uri()).with_message(message))
}

/// Put accounts on a list (an entry each; ones already on it are left).
pub async fn add(api: &Api, arg: &str, inputs: &[String]) -> Result<Vec<Action>> {
  let list = own(api, arg).await?.uri();
  let entries = entries(api, &list).await?;
  bulk::each("follow-group-add", inputs, async |input| {
    let did = identity::did(api, &users::actor(api, input)?).await?;
    let done = Action::done("follow-group-add", &did);
    if entries.iter().any(|(d, _)| *d == did) {
      return Ok(done.with_message("already on the list"));
    }
    let record = json!({"$type": LIST_ITEM, "subject": did, "list": list, "createdAt": api::now()});
    api.create_record(LIST_ITEM, record).await?;
    Ok(done.with_message(format!("list {list}")))
  })
  .await
}

/// Take accounts off a list (their entries are deleted).
pub async fn remove(api: &Api, arg: &str, inputs: &[String]) -> Result<Vec<Action>> {
  let list = own(api, arg).await?.uri();
  let entries = entries(api, &list).await?;
  bulk::each("follow-group-remove", inputs, async |input| {
    let did = identity::did(api, &users::actor(api, input)?).await?;
    let Some((_, entry)) = entries.iter().find(|(d, _)| *d == did) else {
      return Err(Error::not_found(format!("{did} is not on list {list}")));
    };
    api.delete_record(entry).await?;
    Ok(Action::done("follow-group-remove", &did).with_message(format!("list {list}")))
  })
  .await
}

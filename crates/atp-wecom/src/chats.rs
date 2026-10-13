//! What the app can list: its 微信客服 accounts, app group chats it
//! created, departments and their users.

use atp_core::{Collection, Error, Page, PageReq, Result, User, ValueExt, json};

use crate::api::Api;
use crate::parse;
use crate::refs::{self, Target};

/// `chats`: the 微信客服 accounts (`kf:OPEN_KFID`).
pub async fn list(api: &Api, parent: Option<&str>, req: &PageReq) -> Result<Page<Collection>> {
  if let Some(p) = parent {
    return Err(Error::input(format!(
      "WeCom chats have nothing inside them ({p}); `messages kf:OPEN_KFID` reads a 客服 account"
    )));
  }
  let offset: u64 = req
    .cursor
    .as_deref()
    .and_then(|c| c.parse().ok())
    .unwrap_or(0);
  let limit = req.size_within(100) as u64;
  let body = json!({ "offset": offset, "limit": limit });
  let v = api.post("/kf/account/list", body).await?;
  let items: Vec<Collection> = v
    .list("account_list")
    .iter()
    .map(parse::kf_account)
    .collect();
  let next = (items.len() as u64 >= limit).then(|| (offset + limit).to_string());
  Ok(Page::new(items, next))
}

/// An app group chat (`chat:CHATID`).
pub async fn chat(api: &Api, arg: &str) -> Result<Collection> {
  let Target::Chat(id) = refs::target(arg)? else {
    return Err(Error::input(format!(
      "not an app group chat: {arg} (chat:CHATID)"
    )));
  };
  let v = api.get("/appchat/get", &[("chatid", id.clone())]).await?;
  Ok(parse::group_chat(&id, v.at("chat_info")))
}

pub async fn create_chat(
  api: &Api,
  name: &str,
  owner: Option<&str>,
  users: &[String],
) -> Result<Collection> {
  let mut body = json!({ "name": name, "userlist": users });
  if let Some(o) = owner {
    body["owner"] = json!(o);
  }
  let v = api.post("/appchat/create", body).await?;
  Ok(Collection {
    id: format!("chat:{}", v.str("chatid").unwrap_or_default()),
    kind: "group".into(),
    name: name.to_owned(),
    items: Some(users.len() as u64),
    ..Collection::default()
  })
}

/// Members of an app group chat, or the users of a department (`party:ID`).
pub async fn members(api: &Api, arg: &str, _req: &PageReq) -> Result<Page<User>> {
  match refs::target(arg)? {
    Target::Chat(_) => {
      let chat = chat(api, arg).await?;
      let raw = chat.raw.unwrap_or_default();
      let users = raw
        .list("userlist")
        .iter()
        .filter_map(|u| u.as_str())
        .map(|id| User {
          id: id.to_owned(),
          ..User::default()
        })
        .collect();
      Ok(Page::last(users))
    }
    Target::Party(department) => {
      let query = [("department_id", department)];
      let v = api.get("/user/simplelist", &query).await?;
      Ok(Page::last(
        v.list("userlist").iter().map(parse::member).collect(),
      ))
    }
    _ => Err(Error::input(
      "members takes an app group chat (chat:CHATID) or a department (party:ID)",
    )),
  }
}

pub async fn departments(api: &Api, parent: Option<&str>) -> Result<Page<Collection>> {
  let mut query = Vec::new();
  if let Some(p) = parent {
    query.push(("id", p.trim_start_matches("party:").to_owned()));
  }
  let v = api.get("/department/list", &query).await?;
  Ok(Page::last(
    v.list("department").iter().map(parse::department).collect(),
  ))
}

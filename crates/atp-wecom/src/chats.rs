//! What the app can list: its 微信客服 accounts and their conversations
//! (`kf/sync_msg`), app group chats it created, departments and users.
//! WeCom has no API for ordinary chat history (that is 会话存档, a paid
//! add-on with its own SDK).

use atp_core::text::from_secs;
use atp_core::{Collection, Error, Page, PageReq, Post, Result, User, Value, ValueExt, json};

use crate::api::Api;
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
  let v = api
    .post(
      "/kf/account/list",
      json!({ "offset": offset, "limit": limit }),
    )
    .await?;
  let items: Vec<Collection> = v
    .list("account_list")
    .iter()
    .map(|a| Collection {
      id: format!("kf:{}", a.str("open_kfid").unwrap_or_default()),
      kind: "kf".into(),
      name: a.str("name").unwrap_or_default(),
      extra: a
        .str("avatar")
        .map(|x| ("avatar".to_owned(), Value::from(x)))
        .into_iter()
        .collect(),
      raw: Some(a.clone()),
      ..Collection::default()
    })
    .collect();
  let next = (items.len() as u64 >= limit).then(|| (offset + limit).to_string());
  Ok(Page::new(items, next))
}

/// `messages kf:OPEN_KFID`: what customers and servicers exchanged, oldest
/// first, as `kf/sync_msg` hands it out (strictly rate limited without the
/// token of a callback).
pub async fn messages(api: &Api, chat: &str, req: &PageReq) -> Result<Page<Post>> {
  let account = match chat.trim().strip_prefix("kf:") {
    Some(rest) => rest.split('/').next().unwrap_or(rest).to_owned(),
    None => {
      return Err(Error::unsupported("messages").with_hint(
        "WeCom only reads 微信客服 conversations (`messages kf:OPEN_KFID`); other chat history needs 会话存档",
      ));
    }
  };
  let mut body = json!({
    "open_kfid": account,
    "limit": req.size_within(1000),
  });
  if let Some(c) = &req.cursor {
    body["cursor"] = json!(c);
  }
  let v = api.post("/kf/sync_msg", body).await?;
  let posts = v
    .list("msg_list")
    .iter()
    .map(|m| kf_message(&account, m))
    .collect();
  let next = (v.i64("has_more") == Some(1))
    .then(|| v.str("next_cursor"))
    .flatten();
  Ok(Page::new(posts, next))
}

fn kf_message(account: &str, m: &Value) -> Post {
  let kind = m.str("msgtype").unwrap_or_default();
  let text = match kind.as_str() {
    "text" => m.str("text.content"),
    "event" => Some(format!(
      "[{}]",
      m.str("event.event_type").unwrap_or_default()
    )),
    "link" => m.str("link.title"),
    "location" => m.str("location.name"),
    "file" | "image" | "voice" | "video" => Some(format!("[{kind}]")),
    other => Some(format!("[{other}]")),
  };
  let customer = m.str("external_userid").unwrap_or_default();
  let author = match m.i64("origin") {
    Some(5) => m.str("servicer_userid"),
    Some(4) => Some("system".into()),
    _ => Some(customer.clone()),
  };
  let mut post = Post {
    id: format!("kf:{account}/{}", m.str("msgid").unwrap_or_default()),
    kind: if kind == "event" {
      "event".into()
    } else {
      "message".into()
    },
    text,
    author: author.map(|id| User {
      id,
      ..User::default()
    }),
    created_at: m.i64("send_time").and_then(from_secs),
    raw: Some(m.clone()),
    ..Post::default()
  };
  post
    .extra
    .insert("chat".into(), format!("kf:{account}/{customer}").into());
  post.extra.insert("msg_type".into(), kind.into());
  if let Some(media) = m.str(&format!(
    "{}.media_id",
    m.str("msgtype").unwrap_or_default()
  )) {
    post.extra.insert("media_id".into(), media.into());
  }
  post
}

/// An app group chat (`chat:CHATID`).
pub async fn chat(api: &Api, arg: &str) -> Result<Collection> {
  let Target::Chat(id) = refs::target(arg)? else {
    return Err(Error::input(format!(
      "not an app group chat: {arg} (chat:CHATID)"
    )));
  };
  let v = api.get("/appchat/get", &[("chatid", id.clone())]).await?;
  let c = v.at("chat_info");
  Ok(Collection {
    id: format!("chat:{id}"),
    kind: "group".into(),
    name: c.str("name").unwrap_or_default(),
    items: Some(c.list("userlist").len() as u64),
    owner: c.str("owner").map(|id| User {
      id,
      ..User::default()
    }),
    raw: Some(c.clone()),
    ..Collection::default()
  })
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
  let id = v.str("chatid").unwrap_or_default();
  Ok(Collection {
    id: format!("chat:{id}"),
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
      let c = chat(api, arg).await?;
      let users = c
        .raw
        .as_ref()
        .map(|r| {
          r.list("userlist")
            .iter()
            .filter_map(|u| u.as_str())
            .map(|id| User {
              id: id.to_owned(),
              ..User::default()
            })
            .collect()
        })
        .unwrap_or_default();
      Ok(Page::last(users))
    }
    Target::Party(dept) => {
      let v = api
        .get("/user/simplelist", &[("department_id", dept)])
        .await?;
      Ok(Page::last(
        v.list("userlist")
          .iter()
          .map(|u| User {
            id: u.str("userid").unwrap_or_default(),
            name: u.str("name").unwrap_or_default(),
            extra: [("department".to_owned(), u.at("department").clone())]
              .into_iter()
              .collect(),
            ..User::default()
          })
          .collect(),
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
    v.list("department")
      .iter()
      .map(|d| {
        let id = d.str("id").unwrap_or_default();
        Collection {
          id: format!("party:{id}"),
          kind: "department".into(),
          name: d.str("name").unwrap_or_default(),
          extra: [("parent".to_owned(), d.at("parentid").clone())]
            .into_iter()
            .collect(),
          raw: Some(d.clone()),
          ..Collection::default()
        }
      })
      .collect(),
  ))
}

pub async fn user(api: &Api, arg: &str) -> Result<User> {
  let id = arg.trim().trim_start_matches("user:");
  let v = api.get("/user/get", &[("userid", id.to_owned())]).await?;
  Ok(User {
    id: v.str("userid").unwrap_or_else(|| id.to_owned()),
    name: v.str("name").unwrap_or_default(),
    handle: v.str("alias").filter(|a| !a.is_empty()),
    avatar: v.str("avatar").filter(|a| !a.is_empty()),
    bio: v.str("position").filter(|p| !p.is_empty()),
    extra: [
      "department",
      "main_department",
      "open_userid",
      "status",
      "email",
      "biz_mail",
    ]
    .into_iter()
    .filter_map(|k| {
      let x = v.at(k);
      (!x.is_null()).then(|| (k.to_owned(), x.clone()))
    })
    .collect(),
    raw: Some(v.clone()),
    ..User::default()
  })
}

/// The app (`agent/get`), or the bare corp when no agent id is known.
pub async fn whoami(api: &Api) -> Result<User> {
  if api.app().is_none()
    && let Some(bot) = api.bot()
  {
    return Ok(User {
      id: bot.bot_id.clone(),
      name: format!("WeCom AI bot {}", bot.bot_id),
      ..User::default()
    });
  }
  let app = api.require_app()?;
  api.token().await?;
  let Some(agent) = app.agent_id.clone() else {
    return Ok(User {
      id: app.corp_id.clone(),
      name: format!("WeCom corp {}", app.corp_id),
      ..User::default()
    });
  };
  let v = api.get("/agent/get", &[("agentid", agent.clone())]).await?;
  Ok(User {
    id: agent,
    name: v.str("name").unwrap_or_default(),
    avatar: v.str("square_logo_url"),
    bio: v.str("description").filter(|d| !d.is_empty()),
    extra: [("corp_id".to_owned(), Value::from(app.corp_id))]
      .into_iter()
      .collect(),
    raw: Some(v.clone()),
    ..User::default()
  })
}

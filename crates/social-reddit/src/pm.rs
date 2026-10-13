//! Private messages through the chat commands: a conversation is a message
//! thread (`t4_…` of its first message), `messages` shows it, and `send`
//! replies to one or starts a new one with a user (`u/name`) or a
//! subreddit's moderators (`r/name`). Reddit's newer chat is a separate
//! system without a public API.

use social_core::text::from_secs;
use social_core::{
  Action, Collection, Error, Message, Page, PageReq, Post, Result, User, Value, ValueExt,
};

use crate::api::{Api, WWW};
use crate::listing;
use crate::refs::{Target, target, user_url};

/// `t4_<id>` of a message id, its link or a `t4_…` fullname.
fn fullname(arg: &str) -> Option<String> {
  let arg = arg.trim().trim_end_matches('/');
  let id = match arg.find("/message/messages/") {
    Some(i) => &arg[i + "/message/messages/".len()..],
    None => arg.strip_prefix("t4_")?,
  };
  let id = id.split(['/', '?']).next().unwrap_or_default();
  (!id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric())).then(|| format!("t4_{id}"))
}

fn person(name: Option<String>) -> Option<User> {
  let name = name.filter(|n| !n.is_empty() && n != "[deleted]")?;
  Some(User {
    id: name.clone(),
    url: Some(user_url(&name)),
    handle: Some(name.clone()),
    name,
    ..User::default()
  })
}

/// One `t4` message as a post.
fn message(d: &Value) -> Option<Post> {
  let id = d.str("name")?;
  let mut post = Post {
    id: id.clone(),
    kind: "message".into(),
    title: d.str("subject"),
    text: d.str("body"),
    url: Some(format!(
      "{WWW}/message/messages/{}",
      id.trim_start_matches("t4_")
    )),
    author: person(d.str("author")),
    created_at: d.i64("created_utc").and_then(from_secs),
    raw: Some(d.clone()),
    ..Post::default()
  };
  if let Some(to) = d.str("dest") {
    post.extra.insert("to".into(), to.into());
  }
  if d.bool("new") == Some(true) {
    post.extra.insert("unread".into(), true.into());
  }
  let thread = d.str("first_message_name").unwrap_or(id);
  post.extra.insert("chat".into(), thread.into());
  Some(post)
}

/// A thread's messages: the first one and its `replies`, oldest first.
fn thread(root: &Value) -> Vec<Post> {
  let mut out: Vec<Post> = message(root).into_iter().collect();
  out.extend(
    root
      .list("replies.data.children")
      .iter()
      .filter_map(|r| message(r.at("data"))),
  );
  out
}

/// Message threads you received or answered, newest activity first.
pub async fn chats(api: &Api, req: &PageReq) -> Result<Page<Collection>> {
  api.require_login()?;
  let me = api.username().await?;
  let query = listing::paged(vec![("mark", "false".into())], req);
  let v = api.get("/message/messages", &query).await?;
  let chats = v
    .list("data.children")
    .iter()
    .filter_map(|t| {
      let root = t.at("data");
      let msgs = thread(root);
      let first = msgs.first()?;
      let last = msgs.last()?;
      let other = [root.str("author"), root.str("dest")]
        .into_iter()
        .flatten()
        .find(|n| !n.eq_ignore_ascii_case(&me));
      let mut c = Collection {
        id: first.id.clone(),
        kind: "dm".into(),
        name: first.title.clone().unwrap_or_default(),
        description: last.text.clone(),
        url: first.url.clone(),
        items: Some(msgs.len() as u64),
        owner: person(other),
        ..Collection::default()
      };
      if let Some(t) = last.created_at {
        c.extra.insert("last_at".into(), t.to_string().into());
      }
      if msgs.iter().any(|m| m.extra.contains_key("unread")) {
        c.extra.insert("unread".into(), true.into());
      }
      Some(c)
    })
    .collect();
  Ok(Page::new(chats, v.str("data.after")))
}

/// The messages of one thread, newest first.
pub async fn messages(api: &Api, chat: &str) -> Result<Page<Post>> {
  api.require_login()?;
  let Some(name) = fullname(chat) else {
    return Err(Error::input(format!(
      "not a message thread: {chat} (a t4_… id or message link from `chats`)"
    )));
  };
  let id = name.trim_start_matches("t4_");
  let v = api
    .get(
      &format!("/message/messages/{id}"),
      &[("mark", "false".into())],
    )
    .await?;
  let mut msgs: Vec<Post> = v
    .list("data.children")
    .iter()
    .flat_map(|t| thread(t.at("data")))
    .collect();
  msgs.reverse();
  Ok(Page::last(msgs))
}

/// Reply in a thread (`t4_…`), or write to `u/name` / `r/name` (the
/// subreddit's moderators). A new message's subject is the first line.
pub async fn send(api: &Api, chat: &str, m: &Message) -> Result<Action> {
  if !m.files.is_empty() {
    return Err(Error::unsupported(
      "send --attach (Reddit messages are text only)",
    ));
  }
  let chat = chat.trim();
  let thread = m
    .reply_to
    .as_deref()
    .and_then(fullname)
    .or_else(|| fullname(chat));
  if let Some(parent) = thread {
    let form = vec![("thing_id", parent.clone()), ("text", m.text.clone())];
    let v = api.post("/api/comment", form).await?;
    let mut action = Action::done("reply", parent);
    if let Some(id) = v.str("json.data.things.0.data.name") {
      action = action.with_id(id);
    }
    return Ok(action);
  }
  let to = match target(chat)? {
    Target::User(name) => name,
    Target::Subreddit(name) => format!("/r/{name}"),
  };
  let subject: String = m
    .text
    .lines()
    .next()
    .unwrap_or_default()
    .chars()
    .take(100)
    .collect();
  if subject.trim().is_empty() {
    return Err(Error::input(
      "a new message needs text (its first line is the subject)",
    ));
  }
  let form = vec![
    ("to", to.clone()),
    ("subject", subject),
    ("text", m.text.clone()),
  ];
  api.post("/api/compose", form).await?;
  Ok(Action::done("message", to))
}

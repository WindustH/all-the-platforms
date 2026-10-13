//! Messenger conversations of a Page's inbox: listing them, reading one,
//! and answering in it.

use atp_core::{
  Action, Collection, Error, Message, Page, PageReq, Post, Result, ValueExt, chat, json,
};
use atp_meta::graph::{Graph, Params, fields};

use crate::api::Api;
use crate::parse;

pub async fn chats(api: &Api, page: Option<&str>, req: &PageReq) -> Result<Page<Collection>> {
  let page = api.page_id(page).await?;
  let graph = api.page_graph(&page).await?;
  let (items, next) = graph
    .page(
      &format!("{page}/conversations"),
      &fields("id,updated_time,message_count,unread_count,snippet,participants,link"),
      req,
      100,
    )
    .await?;
  let chats = items
    .iter()
    .map(|v| parse::graph::conversation(v, &page))
    .collect();
  Ok(Page::new(chats, next))
}

pub async fn messages(api: &Api, chat: &str, req: &PageReq) -> Result<Page<Post>> {
  let graph = match api.page_id(None).await {
    Ok(page) => api.page_graph(&page).await?,
    Err(_) => api.graph()?,
  };
  let (items, next) = graph
    .page(
      &format!("{chat}/messages"),
      &fields("id,message,from,to,created_time,attachments,sticker"),
      req,
      100,
    )
    .await?;
  let messages = items
    .iter()
    .map(|m| parse::graph::message(chat, m))
    .collect();
  Ok(Page::new(messages, next))
}

/// Answer in a Page conversation (a conversation id `t_…` or a PSID).
pub async fn send(api: &Api, chat: &str, message: &Message) -> Result<Action> {
  if !message.files.is_empty() {
    return Err(Error::unsupported(
      "send --attach on Facebook Pages (send a link instead)",
    ));
  }
  let page = api.page_id(None).await?;
  let graph = api.page_graph(&page).await?;
  let psid = recipient(&graph, &page, chat).await?;
  let mut form: Params = vec![
    ("recipient".into(), json!({"id": psid}).to_string()),
    ("message".into(), json!({"text": message.text}).to_string()),
    ("messaging_type".into(), "RESPONSE".into()),
  ];
  if let Some(reply_to) = &message.reply_to {
    let mid = chat::split_message_id(reply_to).map_or(reply_to.as_str(), |(_, mid)| mid);
    form.push(("reply_to".into(), json!({"mid": mid}).to_string()));
  }
  let v = graph.post(&format!("{page}/messages"), &form).await?;
  Ok(Action::done("send", chat).with_id(v.str("message_id").unwrap_or_default()))
}

/// The person to message: a conversation's other participant, or `chat`
/// itself when it already is a PSID.
async fn recipient(graph: &Graph<'_>, page: &str, chat: &str) -> Result<String> {
  let is_conversation =
    chat.starts_with("t_") || chat.contains('_') && !chat.bytes().all(|b| b.is_ascii_digit());
  if !is_conversation {
    return Ok(chat.to_owned());
  }
  let v = graph.get(chat, &fields("participants")).await?;
  v.list("participants.data")
    .iter()
    .find_map(|p| p.str("id").filter(|id| id != page))
    .ok_or_else(|| Error::not_found("no other participant in this conversation"))
}

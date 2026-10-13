//! Groups: members, joining and leaving, and the list from the server.

use social_core::{Action, Collection, Error, Page, PageReq, Result, User, Value};

use crate::archive::kind_of;
use crate::link::Link;
use crate::refs::jid;
use crate::{parse, refs};

/// A page of an in-memory list, from offset `start`.
fn paged<T>(items: Vec<T>, start: usize, more: bool) -> Page<T> {
  let next = more.then(|| (start + items.len()).to_string());
  Page::new(items, next)
}

pub async fn members(link: &mut Link, chat: &str, page: &PageReq) -> Result<Page<User>> {
  let me = link.me();
  let chat = refs::chat(chat, &link.archive, &me)?;
  if kind_of(&chat) != "group" {
    return Err(Error::input("members lists the members of a group"));
  }
  let meta = link
    .client
    .groups()
    .get_metadata(&jid(&chat)?)
    .await
    .map_err(|e| Error::upstream(format!("group info failed: {e}")))?;
  let users: Vec<User> = meta
    .participants
    .iter()
    .map(|p| {
      let id = p.phone_number.as_ref().unwrap_or(&p.jid).to_string();
      let mut u = User {
        name: link
          .archive
          .name(&id)
          .map(str::to_owned)
          .unwrap_or_else(|| parse::phone(&id)),
        handle: Some(parse::phone(&id)).filter(|h| h.starts_with('+')),
        id,
        ..User::default()
      };
      u.extra.insert(
        "role".into(),
        format!("{:?}", p.participant_type).to_lowercase().into(),
      );
      u
    })
    .collect();
  let start = page.number_or(0) as usize;
  let size = page.size_within(1000);
  let more = users.len() > start + size;
  Ok(paged(
    users.into_iter().skip(start).take(size).collect(),
    start,
    more,
  ))
}

/// Join a group by invite link, or leave one.
pub async fn follow(link: &mut Link, arg: &str, undo: bool) -> Result<Action> {
  if undo {
    let me = link.me();
    let chat = refs::chat(arg, &link.archive, &me)?;
    link
      .client
      .groups()
      .leave(jid(&chat)?)
      .await
      .map_err(|e| Error::upstream(format!("leaving failed: {e}")))?;
    return Ok(Action::done("leave", chat));
  }
  let joined = link
    .client
    .groups()
    .join_with_invite_code(arg)
    .await
    .map_err(|e| {
      Error::upstream(format!("joining failed: {e}"))
        .with_hint("give a chat.whatsapp.com invite link")
    })?;
  let group = joined.group_jid().to_string();
  Ok(Action::done("join", group.clone()).with_id(group))
}

pub async fn groups(link: &mut Link) -> Result<Page<Collection>> {
  let groups = link
    .client
    .groups()
    .get_participating()
    .await
    .map_err(|e| Error::upstream(format!("listing groups failed: {e}")))?;
  let mut items: Vec<Collection> = groups
    .values()
    .map(|g| {
      let mut c = Collection {
        id: g.id.to_string(),
        kind: "group".into(),
        name: g.subject.clone(),
        description: g.description.clone(),
        items: None,
        followers: Some(g.participants.len() as u64),
        ..Collection::default()
      };
      if let Some(t) = g
        .creation_time
        .and_then(|t| social_core::text::from_secs(t as i64))
      {
        c.extra
          .insert("created_at".into(), Value::from(t.to_string()));
      }
      c
    })
    .collect();
  items.sort_by(|a, b| a.name.cmp(&b.name));
  Ok(Page::new(items, None))
}

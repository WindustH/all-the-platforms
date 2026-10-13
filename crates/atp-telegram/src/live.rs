//! `listen`: live events of a user account (the updates Telegram pushes on
//! the connection), or — without an account — new posts of public channels,
//! checked every half minute through the web preview.

use std::collections::BTreeMap;
use std::time::Duration;

use atp_core::output::Stream;
use atp_core::{Ctx, Error, Post, Result};
use grammers_client::client::UpdatesConfiguration;
use grammers_client::session::types::PeerId;
use grammers_client::update::Update;

use crate::api::{Api, Mode};
use crate::mt::{self, Conn, Mt, boxed, error};
use crate::parse;
use crate::refs::{self, Chat};
use crate::web;

const WEB_EVERY: Duration = Duration::from_secs(30);

pub async fn listen(api: &Api, chats: &[String], out: &Stream) -> Result<()> {
  match api.mode()? {
    Mode::User(conn) => {
      let result = user(&api.ctx, &api.mt, &conn, chats, out).await;
      api.mt.save(&api.ctx);
      result
    }
    Mode::Bot(b) => b.listen(&api.ctx, chats, out).await,
    Mode::Web => web(&api.ctx, chats, out).await,
  }
}

async fn user(ctx: &Ctx, mt: &Mt, conn: &Conn, chats: &[String], out: &Stream) -> Result<()> {
  let mut wanted = Vec::new();
  for c in chats {
    wanted.push(mt::peer(conn, &refs::chat(c)?).await?.id);
  }
  // Peers seen while resolving are worth keeping even if the stream never ends.
  mt.save(ctx);
  let updates = conn
    .take_updates()
    .ok_or_else(|| Error::internal("the update channel is already in use"))?;
  let mut stream = conn
    .client
    .stream_updates(updates, UpdatesConfiguration::default())
    .await
    .map_err(boxed)?;
  let pick = |id: PeerId| wanted.is_empty() || wanted.contains(&id);
  loop {
    match stream.next().await.map_err(error)? {
      Update::NewMessage(m) if pick(m.peer_id()) => out.post(&parse::post(&m)),
      Update::MessageEdited(m) if pick(m.peer_id()) => {
        let mut p = parse::post(&m);
        p.kind = "edit".into();
        out.post(&p);
      }
      Update::MessageDeleted(d) => {
        let chat = d.channel_id().map(PeerId::channel_unchecked);
        if !wanted.is_empty() && !chat.is_some_and(|c| wanted.contains(&c)) {
          continue;
        }
        for id in d.messages() {
          out.post(&deleted(chat, *id));
        }
      }
      _ => {}
    }
  }
}

/// A deletion as an event: private chats and basic groups do not say where.
fn deleted(chat: Option<PeerId>, id: i32) -> Post {
  let dialog = chat.map(parse::dialog_id);
  let mut post = Post {
    id: match dialog {
      Some(d) => refs::message_id(None, d, id),
      None => id.to_string(),
    },
    kind: "delete".into(),
    ..Post::default()
  };
  if let Some(d) = dialog {
    post.extra.insert("chat".into(), d.into());
  }
  post.extra.insert("message_id".into(), id.into());
  post
}

/// New posts of public channels, without an account.
async fn web(ctx: &Ctx, chats: &[String], out: &Stream) -> Result<()> {
  let mut names = Vec::new();
  for c in chats {
    match refs::chat(c)? {
      Chat::Username(u) => names.push(u),
      _ => {
        return Err(
          Error::input("without an account only public channels (@name) can be followed")
            .with_hint(ctx.login_hint()),
        );
      }
    }
  }
  if names.is_empty() {
    return Err(Error::input(
      "name the public channels to follow (`listen @channel ...`), or log in",
    ));
  }
  // The newest id per channel; the first round only sets the mark.
  let mut seen: BTreeMap<String, i64> = BTreeMap::new();
  loop {
    for name in &names {
      let page = web::posts(ctx, name, None, None).await?;
      let newest = page
        .items
        .iter()
        .filter_map(|p| p.extra.get("message_id").and_then(|v| v.as_i64()))
        .max();
      if let Some(mark) = seen.get(name) {
        for p in page.items.iter().rev() {
          if p.extra.get("message_id").and_then(|v| v.as_i64()) > Some(*mark) {
            out.post(p);
          }
        }
      }
      if let Some(n) = newest {
        seen.insert(name.clone(), n.max(seen.get(name).copied().unwrap_or(0)));
      }
    }
    tokio::time::sleep(WEB_EVERY).await;
  }
}

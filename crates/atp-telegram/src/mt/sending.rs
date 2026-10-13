//! Sending (text, files, albums, replies), editing, deleting, reactions,
//! pins and forwards.

use atp_core::{Action, Error, Message as Outgoing, Result};
use grammers_client::media::InputMedia;
use grammers_client::message::{InputMessage, InputReactions};

use crate::mt::{self, Conn, error};
use crate::parse;
use crate::refs;
use crate::uploads::is_photo;

pub async fn send(conn: &Conn, chat: &str, message: &Outgoing) -> Result<Action> {
  let chat = refs::chat(chat)?;
  let peer = mt::peer(conn, &chat).await?;
  let reply = match &message.reply_to {
    Some(r) => Some(refs::message(r)?.id),
    None => None,
  };
  let sent = match message.files.as_slice() {
    [] => conn
      .client
      .send_message(
        peer,
        InputMessage::new().text(&message.text).reply_to(reply),
      )
      .await
      .map_err(error)?,
    [file] => {
      let uploaded = upload(conn, file).await?;
      let input = InputMessage::new().text(&message.text).reply_to(reply);
      let input = if is_photo(file) {
        input.photo(uploaded)
      } else {
        input.document(uploaded)
      };
      conn.client.send_message(peer, input).await.map_err(error)?
    }
    files => {
      let mut album = Vec::new();
      for (i, file) in files.iter().enumerate() {
        let uploaded = upload(conn, file).await?;
        let mut media = InputMedia::new();
        if i == 0 {
          media = media.caption(&message.text).reply_to(reply);
        }
        album.push(if is_photo(file) {
          media.photo(uploaded)
        } else {
          media.document(uploaded)
        });
      }
      conn
        .client
        .send_album(peer, album)
        .await
        .map_err(error)?
        .into_iter()
        .flatten()
        .next()
        .ok_or_else(|| Error::upstream("Telegram returned no message for the album"))?
    }
  };
  let post = parse::post(&sent);
  let mut action = Action::done("send", chat.to_string()).with_id(&post.id);
  if let Some(url) = post.url {
    action = action.with_url(url);
  }
  Ok(action)
}

pub(super) async fn upload(
  conn: &Conn,
  path: &std::path::Path,
) -> Result<grammers_client::media::Uploaded> {
  conn
    .client
    .upload_file(path)
    .await
    .map_err(|e| Error::input(format!("cannot upload {}: {e}", path.display())))
}

pub async fn edit(conn: &Conn, arg: &str, text: &str) -> Result<Action> {
  let m = refs::message(arg)?;
  let peer = mt::peer(conn, &m.chat).await?;
  conn
    .client
    .edit_message(peer, m.id, InputMessage::new().text(text))
    .await
    .map_err(error)?;
  Ok(Action::done("edit", arg))
}

pub async fn delete(conn: &Conn, arg: &str) -> Result<Action> {
  let m = refs::message(arg)?;
  let peer = mt::peer(conn, &m.chat).await?;
  conn
    .client
    .delete_messages(peer, &[m.id])
    .await
    .map_err(error)?;
  Ok(Action::done("delete", arg))
}

pub async fn react(conn: &Conn, arg: &str, emoji: &str, undo: bool) -> Result<Action> {
  let m = refs::message(arg)?;
  let peer = mt::peer(conn, &m.chat).await?;
  let reactions = if undo {
    InputReactions::remove()
  } else {
    InputReactions::emoticon(emoji)
  };
  conn
    .client
    .send_reactions(peer, m.id, reactions)
    .await
    .map_err(error)?;
  Ok(Action::done(if undo { "unreact" } else { "react" }, arg).with_message(emoji))
}

pub async fn pin(conn: &Conn, arg: &str, undo: bool) -> Result<Action> {
  let m = refs::message(arg)?;
  let peer = mt::peer(conn, &m.chat).await?;
  if undo {
    conn.client.unpin_message(peer, m.id).await.map_err(error)?;
  } else {
    conn.client.pin_message(peer, m.id).await.map_err(error)?;
  }
  Ok(Action::done(if undo { "unpin" } else { "pin" }, arg))
}

pub async fn forward(conn: &Conn, arg: &str, to: &str) -> Result<Action> {
  let m = refs::message(arg)?;
  let from = mt::peer(conn, &m.chat).await?;
  let to_chat = refs::chat(to)?;
  let target = mt::peer(conn, &to_chat).await?;
  let sent = conn
    .client
    .forward_messages(target, &[m.id], from)
    .await
    .map_err(error)?;
  let mut action = Action::done("forward", arg);
  if let Some(post) = sent.into_iter().flatten().next().map(|m| parse::post(&m)) {
    action = action.with_id(&post.id);
    if let Some(url) = post.url {
      action = action.with_url(url);
    }
  }
  Ok(action)
}

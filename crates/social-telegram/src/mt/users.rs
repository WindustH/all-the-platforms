//! The account and other users.

use grammers_client::peer::Peer;
use grammers_client::tl;
use social_core::{Result, User};

use crate::mt::{self, Conn, error};
use crate::parse;
use crate::refs::{self};

pub async fn whoami(conn: &Conn) -> Result<User> {
  let me = conn.client.get_me().await.map_err(error)?;
  Ok(parse::user(&me))
}

/// A user, group or channel with its full profile (bio, member count).
pub async fn user(conn: &Conn, arg: &str) -> Result<User> {
  let peer_ref = mt::peer(conn, &refs::chat(arg)?).await?;
  let peer = conn.client.resolve_peer(peer_ref).await.map_err(error)?;
  let mut out = parse::peer_user(&peer);
  match &peer {
    Peer::User(_) => {
      let request = tl::functions::users::GetFullUser {
        id: peer_ref.into(),
      };
      if let Ok(tl::enums::users::UserFull::Full(full)) = conn.client.invoke(&request).await {
        let tl::enums::UserFull::Full(f) = full.full_user;
        out.bio = f.about.filter(|a| !a.is_empty());
        out
          .extra
          .insert("common_chats".into(), f.common_chats_count.into());
      }
    }
    _ if parse::channel(&peer).is_some() => {
      let request = tl::functions::channels::GetFullChannel {
        channel: peer_ref.into(),
      };
      if let Ok(tl::enums::messages::ChatFull::Full(full)) = conn.client.invoke(&request).await
        && let tl::enums::ChatFull::ChannelFull(f) = full.full_chat
      {
        out.bio = Some(f.about).filter(|a| !a.is_empty());
        out.stats.followers = f.participants_count.map(|n| n as u64);
        if let Some(online) = f.online_count {
          out.stats.other.insert("online".into(), online as u64);
        }
        if let Some(linked) = f.linked_chat_id {
          out
            .extra
            .insert("linked_chat".into(), (-1_000_000_000_000 - linked).into());
        }
        if f.can_view_stats {
          out.extra.insert("can_view_stats".into(), true.into());
        }
      }
    }
    _ => {}
  }
  Ok(out)
}

//! The MTProto session grammers works with, kept in memory and written back
//! by [`crate::mt`]: the authorization keys and home datacenter go into the
//! social-cli session file (private, like cookies), the peer cache (access
//! hashes of chats and users seen so far) into the private cache.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use grammers_session::types::{DcOption, PeerId, PeerInfo, UpdateState, UpdatesState};
use grammers_session::{BoxFuture, Session, SessionData};
use serde::{Deserialize, Serialize};

/// What a login leaves: the home datacenter and the authorization keys.
#[derive(Serialize, Deserialize)]
pub struct Keys {
  pub home_dc: i32,
  pub dc_options: Vec<DcOption>,
}

#[derive(Debug)]
pub struct Poisoned;

impl std::fmt::Display for Poisoned {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str("session lock is poisoned")
  }
}

impl std::error::Error for Poisoned {}

pub struct MtSession {
  data: Mutex<SessionData>,
  keys_changed: AtomicBool,
  peers_changed: AtomicBool,
}

impl MtSession {
  pub fn new(keys: Option<Keys>, peers: Vec<PeerInfo>) -> Self {
    let mut data = SessionData::default();
    if let Some(keys) = keys {
      data.home_dc = keys.home_dc;
      for option in keys.dc_options {
        data.dc_options.insert(option.id, option);
      }
    }
    data.peer_infos = peers.into_iter().map(|p| (p.id(), p)).collect();
    Self {
      data: Mutex::new(data),
      keys_changed: AtomicBool::new(false),
      peers_changed: AtomicBool::new(false),
    }
  }

  fn data(&self) -> Result<std::sync::MutexGuard<'_, SessionData>, Poisoned> {
    self.data.lock().map_err(|_| Poisoned)
  }

  pub fn keys(&self) -> Option<Keys> {
    let data = self.data().ok()?;
    let mut dc_options: Vec<DcOption> = data.dc_options.values().cloned().collect();
    dc_options.sort_by_key(|o| o.id);
    Some(Keys {
      home_dc: data.home_dc,
      dc_options,
    })
  }

  pub fn peers(&self) -> Vec<PeerInfo> {
    self
      .data()
      .map(|d| d.peer_infos.values().cloned().collect())
      .unwrap_or_default()
  }

  /// Whether keys / peers changed since the last call.
  pub fn take_changes(&self) -> (bool, bool) {
    (
      self.keys_changed.swap(false, Ordering::Relaxed),
      self.peers_changed.swap(false, Ordering::Relaxed),
    )
  }

  pub fn self_user(&self) -> Option<PeerInfo> {
    let data = self.data().ok()?;
    find_self(&data.peer_infos)
  }
}

fn find_self(peers: &HashMap<PeerId, PeerInfo>) -> Option<PeerInfo> {
  peers
    .values()
    .find(|p| {
      matches!(
        p,
        PeerInfo::User {
          is_self: Some(true),
          ..
        }
      )
    })
    .cloned()
}

impl Session for MtSession {
  type Error = Poisoned;

  fn home_dc_id(&self) -> Result<i32, Poisoned> {
    Ok(self.data()?.home_dc)
  }

  fn set_home_dc_id(&self, dc_id: i32) -> BoxFuture<'_, Result<(), Poisoned>> {
    Box::pin(async move {
      self.data()?.home_dc = dc_id;
      self.keys_changed.store(true, Ordering::Relaxed);
      Ok(())
    })
  }

  fn dc_option(&self, dc_id: i32) -> Result<Option<DcOption>, Poisoned> {
    Ok(self.data()?.dc_options.get(&dc_id).cloned())
  }

  fn set_dc_option(&self, dc_option: &DcOption) -> BoxFuture<'_, Result<(), Poisoned>> {
    let dc_option = dc_option.clone();
    Box::pin(async move {
      self.data()?.dc_options.insert(dc_option.id, dc_option);
      self.keys_changed.store(true, Ordering::Relaxed);
      Ok(())
    })
  }

  fn peer(&self, peer: PeerId) -> BoxFuture<'_, Result<Option<PeerInfo>, Poisoned>> {
    Box::pin(async move {
      let data = self.data()?;
      if peer == PeerId::self_user() {
        return Ok(find_self(&data.peer_infos));
      }
      Ok(data.peer_infos.get(&peer).cloned())
    })
  }

  fn cache_peer(&self, peer: &PeerInfo) -> BoxFuture<'_, Result<(), Poisoned>> {
    let peer = peer.clone();
    Box::pin(async move {
      let mut data = self.data()?;
      let changed = match data.peer_infos.get_mut(&peer.id()) {
        Some(known) => {
          let before = known.clone();
          known.extend_info(&peer);
          *known != before
        }
        None => {
          data.peer_infos.insert(peer.id(), peer);
          true
        }
      };
      if changed {
        self.peers_changed.store(true, Ordering::Relaxed);
      }
      Ok(())
    })
  }

  fn updates_state(&self) -> BoxFuture<'_, Result<UpdatesState, Poisoned>> {
    Box::pin(async move { Ok(self.data()?.updates_state.clone()) })
  }

  fn set_update_state(&self, update: UpdateState) -> BoxFuture<'_, Result<(), Poisoned>> {
    Box::pin(async move {
      let mut data = self.data()?;
      let state = &mut data.updates_state;
      match update {
        UpdateState::All(all) => *state = all,
        UpdateState::Primary { pts, date, seq } => {
          state.pts = pts;
          state.date = date;
          state.seq = seq;
        }
        UpdateState::Secondary { qts } => state.qts = qts,
        UpdateState::Channel { id, pts } => {
          state.channels.retain(|c| c.id != id);
          state
            .channels
            .push(grammers_session::types::ChannelState { id, pts });
        }
      }
      Ok(())
    })
  }
}

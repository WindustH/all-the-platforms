//! What the linked device has received, kept on disk.
//!
//! WhatsApp keeps no history on its servers: a linked device gets the
//! phone's recent chats once (history sync, right after pairing), then live
//! messages, plus older history on request while the phone is online. So
//! every connection feeds this archive, and `chats` / `messages` read it.
//! Files (owner-only): `chats.json`, `names.json` and one `m/<chat>.json`
//! per chat.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::store::write_private;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ChatEntry {
  pub jid: String,
  pub name: Option<String>,
  /// `dm`, `group`, `channel` (newsletter) or `broadcast`.
  pub kind: String,
  pub unread: u32,
  /// Unix seconds of the last message.
  pub last: i64,
  pub archived: bool,
  pub pinned: bool,
  pub muted_until: i64,
  pub description: Option<String>,
}

/// Downloadable media: what `DownloadParams::encrypted` needs, base64 keys.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MediaRef {
  /// `image`, `video`, `audio`, `voice`, `document`, `sticker`, `gif`.
  pub kind: String,
  pub mime: Option<String>,
  pub file_name: Option<String>,
  pub direct_path: String,
  pub key: String,
  pub sha256: String,
  pub enc_sha256: String,
  pub length: u64,
  pub width: Option<u32>,
  pub height: Option<u32>,
  pub seconds: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Record {
  pub id: String,
  pub chat: String,
  pub sender: String,
  pub from_me: bool,
  /// Unix seconds.
  pub ts: i64,
  pub push_name: Option<String>,
  /// `text`, `image`, `video`, `audio`, `voice`, `document`, `sticker`,
  /// `location`, `contact`, `poll`, `system` ...
  pub kind: String,
  pub text: Option<String>,
  pub media: Option<MediaRef>,
  /// Id of the message this one quotes.
  pub quoted: Option<String>,
  pub forwarded: bool,
  pub edited: bool,
  pub deleted: bool,
  /// Sender → emoji.
  pub reactions: BTreeMap<String, String>,
}

pub struct Archive {
  dir: PathBuf,
  chats: BTreeMap<String, ChatEntry>,
  names: BTreeMap<String, String>,
  loaded: HashMap<String, Vec<Record>>,
  touched: BTreeSet<String>,
  index_changed: bool,
}

impl Archive {
  pub fn open(dir: &Path) -> Self {
    let read = |name: &str| std::fs::read(dir.join(name)).ok();
    Self {
      dir: dir.to_owned(),
      chats: read("chats.json")
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default(),
      names: read("names.json")
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default(),
      loaded: HashMap::new(),
      touched: BTreeSet::new(),
      index_changed: false,
    }
  }

  /// Chats, most recent first (pinned ones on top).
  pub fn chats(&self) -> Vec<&ChatEntry> {
    let mut chats: Vec<&ChatEntry> = self.chats.values().collect();
    chats.sort_by(|a, b| b.pinned.cmp(&a.pinned).then(b.last.cmp(&a.last)));
    chats
  }

  pub fn chat(&self, jid: &str) -> Option<&ChatEntry> {
    self.chats.get(jid)
  }

  pub fn chat_mut(&mut self, jid: &str) -> &mut ChatEntry {
    self.index_changed = true;
    self
      .chats
      .entry(jid.to_owned())
      .or_insert_with(|| ChatEntry {
        jid: jid.to_owned(),
        kind: kind_of(jid).into(),
        ..ChatEntry::default()
      })
  }

  /// A display name: the chat's own, a contact name or a push name.
  pub fn name(&self, jid: &str) -> Option<&str> {
    let user = bare(jid);
    self
      .chats
      .get(jid)
      .and_then(|c| c.name.as_deref())
      .or_else(|| self.names.get(jid).map(String::as_str))
      .or_else(|| self.names.get(&user).map(String::as_str))
  }

  /// Remember a name for a user; `contact` names (address book) win over
  /// push names the user chose.
  pub fn set_name(&mut self, jid: &str, name: &str, contact: bool) {
    let name = name.trim();
    if name.is_empty() {
      return;
    }
    let key = bare(jid);
    if !contact && self.names.contains_key(&key) && self.names_from_contacts(&key) {
      return;
    }
    if self.names.get(&key).map(String::as_str) != Some(name) {
      self.names.insert(key.clone(), name.to_owned());
      if contact {
        self.names.insert(format!("~{key}"), String::new());
      }
      self.index_changed = true;
    }
  }

  fn names_from_contacts(&self, key: &str) -> bool {
    self.names.contains_key(&format!("~{key}"))
  }

  /// Every name the archive knows, for resolving chats given by name.
  pub fn find(&self, query: &str) -> Vec<String> {
    let q = query.to_lowercase();
    let mut hits: Vec<String> = self
      .chats
      .values()
      .filter(|c| {
        c.name
          .as_deref()
          .or_else(|| self.names.get(&bare(&c.jid)).map(String::as_str))
          .is_some_and(|n| n.to_lowercase() == q)
      })
      .map(|c| c.jid.clone())
      .collect();
    if hits.is_empty() {
      hits = self
        .chats
        .values()
        .filter(|c| {
          c.name
            .as_deref()
            .or_else(|| self.names.get(&bare(&c.jid)).map(String::as_str))
            .is_some_and(|n| n.to_lowercase().contains(&q))
        })
        .map(|c| c.jid.clone())
        .collect();
    }
    hits
  }

  fn messages_mut(&mut self, chat: &str) -> &mut Vec<Record> {
    if !self.loaded.contains_key(chat) {
      let records = std::fs::read(self.file(chat))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
      self.loaded.insert(chat.to_owned(), records);
    }
    self.loaded.get_mut(chat).expect("loaded above")
  }

  /// A chat's messages, oldest first.
  pub fn messages(&mut self, chat: &str) -> &[Record] {
    self.messages_mut(chat)
  }

  /// Add or replace a message (by id); keeps the chat's order by time.
  pub fn put(&mut self, record: Record) {
    let chat = record.chat.clone();
    let ts = record.ts;
    let list = self.messages_mut(&chat);
    match list.iter_mut().find(|r| r.id == record.id) {
      Some(old) => {
        let reactions = std::mem::take(&mut old.reactions);
        *old = Record {
          reactions,
          ..record
        };
      }
      None => {
        let at = list.partition_point(|r| r.ts <= ts);
        list.insert(at, record);
      }
    }
    self.touched.insert(chat.clone());
    let entry = self.chat_mut(&chat);
    entry.last = entry.last.max(ts);
  }

  /// Apply a change to one message, when the archive has it.
  pub fn update(&mut self, chat: &str, id: &str, f: impl FnOnce(&mut Record)) -> bool {
    let hit = self
      .messages_mut(chat)
      .iter_mut()
      .find(|r| r.id == id)
      .map(f)
      .is_some();
    if hit {
      self.touched.insert(chat.to_owned());
    }
    hit
  }

  pub fn find_message(&mut self, chat: &str, id: &str) -> Option<Record> {
    self.messages_mut(chat).iter().find(|r| r.id == id).cloned()
  }

  /// Every archived chat's messages matching `pred`, newest first.
  pub fn search(&mut self, pred: impl Fn(&Record) -> bool) -> Vec<Record> {
    let chats: Vec<String> = self.chats.keys().cloned().collect();
    let mut hits: Vec<Record> = Vec::new();
    for chat in chats {
      hits.extend(self.messages_mut(&chat).iter().filter(|r| pred(r)).cloned());
    }
    hits.sort_by_key(|r| std::cmp::Reverse(r.ts));
    hits
  }

  fn file(&self, chat: &str) -> PathBuf {
    let safe: String = chat
      .chars()
      .map(|c| {
        if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '@') {
          c
        } else {
          '_'
        }
      })
      .collect();
    self.dir.join("m").join(format!("{safe}.json"))
  }

  /// Write what changed.
  pub fn save(&mut self) -> std::io::Result<()> {
    for chat in std::mem::take(&mut self.touched) {
      if let Some(records) = self.loaded.get(&chat) {
        write_private(&self.file(&chat), &serde_json::to_vec(records)?)?;
      }
    }
    if std::mem::take(&mut self.index_changed) {
      write_private(
        &self.dir.join("chats.json"),
        &serde_json::to_vec(&self.chats)?,
      )?;
      write_private(
        &self.dir.join("names.json"),
        &serde_json::to_vec(&self.names)?,
      )?;
    }
    Ok(())
  }
}

/// A JID without its device part (`123:4@s.whatsapp.net` →
/// `123@s.whatsapp.net`).
pub fn bare(jid: &str) -> String {
  match (jid.find(':'), jid.find('@')) {
    (Some(colon), Some(at)) if colon < at => format!("{}{}", &jid[..colon], &jid[at..]),
    _ => jid.to_owned(),
  }
}

pub fn kind_of(jid: &str) -> &'static str {
  if jid.ends_with("@g.us") {
    "group"
  } else if jid.ends_with("@newsletter") {
    "channel"
  } else if jid.ends_with("@broadcast") {
    "broadcast"
  } else {
    "dm"
  }
}

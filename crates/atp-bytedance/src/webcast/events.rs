//! What the messages of a room say, read the same way on Douyin and
//! TikTok: chat, gifts, likes, arrivals, follows, shares, audience counts
//! and the end of the live.

use atp_core::proto::Msg;

use super::Message;

/// Who sent a message: a webcast `User` (`id` 1, `nickname` 3,
/// `display_id` 38, `sec_uid` 46, `id_str` 1028).
pub struct Sender {
  /// `id_str`, else `id`.
  pub id: Option<String>,
  pub nickname: String,
  pub display_id: Option<String>,
  pub sec_uid: Option<String>,
}

/// One message as an event of the room.
pub struct Event {
  /// `message`, `gift`, `like`, `enter`, `follow`, `share`, `stats` or
  /// `offline`.
  pub kind: &'static str,
  pub text: Option<String>,
  pub sender: Option<Sender>,
  /// `common.createTime`, in milliseconds.
  pub at_ms: Option<u64>,
  /// Counters the message carries (`count`, `likes`, `viewers` …).
  pub counts: Vec<(&'static str, u64)>,
}

/// The event of a message, `None` for the kinds `listen` leaves out.
pub fn event(m: &Message) -> Option<Event> {
  let msg = Msg(&m.payload);
  let (kind, text, sender) = match m.method.as_str() {
    "WebcastChatMessage" => ("message", msg.string(3), 2),
    "WebcastGiftMessage" => ("gift", Some(gift(msg)), 7),
    "WebcastLikeMessage" => ("like", msg.varint(2).map(|n| format!("× {n}")), 5),
    "WebcastMemberMessage" => ("enter", Some("joined".into()), 2),
    "WebcastSocialMessage" if msg.varint(4) == Some(1) => ("follow", Some("followed".into()), 2),
    "WebcastSocialMessage" => ("share", Some("shared the room".into()), 2),
    "WebcastRoomStatsMessage" => ("stats", msg.string(4), 0),
    "WebcastControlMessage" if msg.varint(2) == Some(3) => {
      ("offline", Some("live ended".into()), 0)
    }
    _ => return None,
  };
  Some(Event {
    kind,
    text,
    sender: sender_of(msg.message(sender)),
    at_ms: msg.message(1).varint(4),
    counts: counts(kind, msg),
  })
}

/// `name × count` of a gift (`gift` 15 → `name` 16, `comboCount` 6).
fn gift(msg: Msg) -> String {
  let name = msg.message(15).string(16).unwrap_or_else(|| "gift".into());
  format!("{name} × {}", msg.varint(6).unwrap_or(1).max(1))
}

/// The counters a message of `kind` carries.
fn counts(kind: &str, msg: Msg) -> Vec<(&'static str, u64)> {
  let fields: &[(&str, u32)] = match kind {
    "gift" => &[("count", 6)],
    "like" => &[("likes", 2), ("likes_total", 3)],
    "enter" => &[("audience", 3)],
    "stats" => &[("viewers", 5), ("viewers_total", 9)],
    _ => &[],
  };
  fields
    .iter()
    .filter_map(|(name, number)| Some((*name, msg.varint(*number)?)))
    .collect()
}

/// A sender with a nickname; `None` for system messages.
fn sender_of(u: Msg) -> Option<Sender> {
  Some(Sender {
    nickname: u.string(3)?,
    id: u
      .string(1028)
      .or_else(|| u.varint(1).map(|id| id.to_string())),
    display_id: u.string(38),
    sec_uid: u.string(46),
  })
}

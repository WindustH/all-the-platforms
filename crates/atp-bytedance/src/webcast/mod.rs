//! The webcast (live room) push protocol of Douyin and TikTok: protobuf
//! `PushFrame`s whose payload is a gzip `Response` holding the room's
//! messages (`WebcastChatMessage` …). Decoded by hand for the few fields
//! `listen` reads; field numbers as in `cv-cat/catbus`
//! (`static/douyin/Live.proto`, `src/platforms/tiktok/web/wire.ts`, MIT).
//!
//! [`event`] reads the messages both sites send alike, [`frontier`] signs
//! the connection.

mod events;
pub mod frontier;

use std::io::Read;

use atp_core::proto::{Msg, put_bytes, put_int};
use flate2::read::GzDecoder;

pub use events::{Event, Sender, event};

/// A `PushFrame`: `logId` (2), `payloadEncoding` (6, TikTok's `pb`; Douyin
/// leaves it out when `encoding` is empty), `payloadType` (7), `payload` (8).
pub fn frame(log_id: u64, encoding: &str, kind: &str, payload: &[u8]) -> Vec<u8> {
  let mut out = Vec::new();
  if log_id != 0 {
    put_int(&mut out, 2, log_id);
  }
  if !encoding.is_empty() {
    put_bytes(&mut out, 6, encoding.as_bytes());
  }
  put_bytes(&mut out, 7, kind.as_bytes());
  if !payload.is_empty() {
    put_bytes(&mut out, 8, payload);
  }
  out
}

/// One message of a pushed batch: `Message.method` (1), `payload` (2),
/// `msgId` (3).
pub struct Message {
  pub method: String,
  pub id: u64,
  pub payload: Vec<u8>,
}

/// A pushed frame's messages, and the acknowledgement to send back when
/// the server asks for one.
pub struct Batch {
  pub messages: Vec<Message>,
  pub ack: Option<Vec<u8>>,
}

/// Decode a binary frame; heartbeats and frames without a payload are
/// `None`. The acknowledgement is framed with `encoding` (see [`frame`]).
pub fn decode(data: &[u8], encoding: &str) -> Option<Batch> {
  let push = Msg(data);
  let kind = push.string(7).unwrap_or_default();
  if kind == "hb" || kind == "ack" {
    return None;
  }
  let payload = inflate(push.bytes(8)?)?;
  // `Response`: `messagesList` (1), `internalExt` (5), `needAck` (9).
  let resp = Msg(&payload);
  let messages = resp
    .repeated(1)
    .into_iter()
    .map(|m| Message {
      method: m.string(1).unwrap_or_default(),
      id: m.varint(3).unwrap_or_default(),
      payload: m.bytes(2).unwrap_or_default().to_vec(),
    })
    .collect();
  let ack = (resp.varint(9) == Some(1)).then(|| {
    let internal_ext = resp.string(5).unwrap_or_default();
    let log_id = push.varint(2).unwrap_or(0);
    frame(log_id, encoding, "ack", internal_ext.as_bytes())
  });
  Some(Batch { messages, ack })
}

/// The payload, gunzipped when it is gzip.
fn inflate(payload: &[u8]) -> Option<Vec<u8>> {
  if !payload.starts_with(&[0x1f, 0x8b]) {
    return Some(payload.to_vec());
  }
  let mut out = Vec::new();
  GzDecoder::new(payload).read_to_end(&mut out).ok()?;
  Some(out)
}

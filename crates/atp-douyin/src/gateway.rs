//! The webcast websocket's frames: protobuf `PushFrame`s whose payload is a
//! gzip `Response` holding the room's messages (`WebcastChatMessage` …).
//! Decoded by hand for the few fields `listen` reads; field numbers as in
//! `cv-cat/catbus` (`static/douyin/Live.proto`, MIT).

use std::io::Read;

use flate2::read::GzDecoder;

/// One protobuf message, its fields read on demand.
#[derive(Clone, Copy, Default)]
pub struct Msg<'a>(pub &'a [u8]);

enum Field<'a> {
  Varint(u64),
  Bytes(&'a [u8]),
}

impl<'a> Msg<'a> {
  /// Every field in order; reading stops at the first malformed one.
  fn fields(self) -> impl Iterator<Item = (u32, Field<'a>)> {
    let mut rest = self.0;
    std::iter::from_fn(move || {
      let tag = varint(&mut rest)?;
      let field = match tag & 7 {
        0 => Field::Varint(varint(&mut rest)?),
        2 => {
          let len = usize::try_from(varint(&mut rest)?).ok()?;
          let (bytes, tail) = (rest.get(..len)?, rest.get(len..)?);
          rest = tail;
          Field::Bytes(bytes)
        }
        1 => skip(&mut rest, 8)?,
        5 => skip(&mut rest, 4)?,
        _ => return None,
      };
      Some(((tag >> 3) as u32, field))
    })
  }

  pub fn varint(self, number: u32) -> Option<u64> {
    self.fields().fold(None, |found, (n, f)| match f {
      Field::Varint(v) if n == number => Some(v),
      _ => found,
    })
  }

  pub fn bytes(self, number: u32) -> Option<&'a [u8]> {
    self.fields().fold(None, |found, (n, f)| match f {
      Field::Bytes(b) if n == number => Some(b),
      _ => found,
    })
  }

  pub fn string(self, number: u32) -> Option<String> {
    self
      .bytes(number)
      .map(|b| String::from_utf8_lossy(b).into_owned())
      .filter(|s| !s.is_empty())
  }

  /// An embedded message; empty when absent.
  pub fn message(self, number: u32) -> Msg<'a> {
    Msg(self.bytes(number).unwrap_or_default())
  }

  pub fn repeated(self, number: u32) -> Vec<Msg<'a>> {
    self
      .fields()
      .filter_map(|(n, f)| match f {
        Field::Bytes(b) if n == number => Some(Msg(b)),
        _ => None,
      })
      .collect()
  }
}

fn varint(rest: &mut &[u8]) -> Option<u64> {
  let mut value = 0u64;
  for shift in (0..64).step_by(7) {
    let (&byte, tail) = rest.split_first()?;
    *rest = tail;
    value |= u64::from(byte & 0x7f) << shift;
    if byte & 0x80 == 0 {
      return Some(value);
    }
  }
  None
}

fn skip<'a>(rest: &mut &'a [u8], len: usize) -> Option<Field<'a>> {
  *rest = rest.get(len..)?;
  Some(Field::Varint(0))
}

fn put_varint(out: &mut Vec<u8>, mut value: u64) {
  while value >= 0x80 {
    out.push((value as u8) | 0x80);
    value >>= 7;
  }
  out.push(value as u8);
}

fn put_bytes(out: &mut Vec<u8>, number: u32, bytes: &[u8]) {
  put_varint(out, u64::from(number << 3 | 2));
  put_varint(out, bytes.len() as u64);
  out.extend_from_slice(bytes);
}

/// A `PushFrame`: `logId` (2), `payloadType` (7), `payload` (8).
fn frame(log_id: u64, kind: &str, payload: &[u8]) -> Vec<u8> {
  let mut out = Vec::new();
  if log_id != 0 {
    put_varint(&mut out, 2 << 3);
    put_varint(&mut out, log_id);
  }
  put_bytes(&mut out, 7, kind.as_bytes());
  if !payload.is_empty() {
    put_bytes(&mut out, 8, payload);
  }
  out
}

/// The frame that keeps the connection open.
pub fn heartbeat() -> Vec<u8> {
  frame(0, "hb", &[])
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

/// Decode a binary frame; heartbeats and frames without a payload are `None`.
pub fn decode(data: &[u8]) -> Option<Batch> {
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
    frame(push.varint(2).unwrap_or(0), "ack", internal_ext.as_bytes())
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

//! The live danmaku socket's protocol. Every frame is a protobuf
//! `SocketMessage {payloadType = 1, compressionType = 2, payload = 3}`;
//! payloads may be gzipped (2) or AES-128-CBC encrypted (3). Up: enter the
//! room (200) with the token of `websocketinfo`, heartbeats (1), exit
//! (202). Down: the enter ack (300) with the heartbeat interval, errors
//! (103) and feed pushes (310) of comments, likes, gifts, shares and
//! notices.

use std::io::Read;

use aes::Aes128;
use cbc::cipher::block_padding::NoPadding;
use cbc::cipher::{BlockDecryptMut, KeyIvInit};
use flate2::read::{DeflateDecoder, GzDecoder};

pub const CS_HEARTBEAT: u64 = 1;
pub const CS_ENTER_ROOM: u64 = 200;
pub const CS_USER_EXIT: u64 = 202;
pub const SC_ERROR: u64 = 103;
pub const SC_ENTER_ROOM_ACK: u64 = 300;
pub const SC_FEED_PUSH: u64 = 310;

const AES_KEY: &[u8; 16] = b"PPbzKKL7NB15leYy";
const AES_IV: &[u8; 16] = b"JRODKJiolJ9xqso0";

// ── encoding ─────────────────────────────────────────────────────────────

fn varint(out: &mut Vec<u8>, mut v: u64) {
  while v >= 0x80 {
    out.push((v as u8) | 0x80);
    v >>= 7;
  }
  out.push(v as u8);
}

fn int_field(out: &mut Vec<u8>, field: u64, v: u64) {
  varint(out, field << 3);
  varint(out, v);
}

fn bytes_field(out: &mut Vec<u8>, field: u64, bytes: &[u8]) {
  varint(out, (field << 3) | 2);
  varint(out, bytes.len() as u64);
  out.extend_from_slice(bytes);
}

fn envelope(kind: u64, payload: &[u8]) -> Vec<u8> {
  let mut out = Vec::new();
  int_field(&mut out, 1, kind);
  bytes_field(&mut out, 3, payload);
  out
}

/// `CSWebEnterRoom {token = 1, liveStreamId = 2, reconnectCount = 3}`.
pub fn enter_room(token: &str, stream: &str, reconnects: u64) -> Vec<u8> {
  let mut body = Vec::new();
  bytes_field(&mut body, 1, token.as_bytes());
  bytes_field(&mut body, 2, stream.as_bytes());
  if reconnects > 0 {
    int_field(&mut body, 3, reconnects);
  }
  envelope(CS_ENTER_ROOM, &body)
}

/// `CSWebHeartbeat {timestamp = 1}`.
pub fn heartbeat(now_ms: u64) -> Vec<u8> {
  let mut body = Vec::new();
  int_field(&mut body, 1, now_ms);
  envelope(CS_HEARTBEAT, &body)
}

/// `CSWebUserExit {time = 1}`.
pub fn exit(now_ms: u64) -> Vec<u8> {
  let mut body = Vec::new();
  int_field(&mut body, 1, now_ms);
  envelope(CS_USER_EXIT, &body)
}

// ── decoding ─────────────────────────────────────────────────────────────

enum Wire<'a> {
  Int(u64),
  Bytes(&'a [u8]),
}

fn read_varint(buf: &[u8], pos: &mut usize) -> Option<u64> {
  let mut value = 0u64;
  for shift in (0..64).step_by(7) {
    let byte = *buf.get(*pos)?;
    *pos += 1;
    value |= u64::from(byte & 0x7f) << shift;
    if byte < 0x80 {
      return Some(value);
    }
  }
  None
}

/// Top-level `(field, value)` pairs of a message; stops at malformed input.
fn fields(buf: &[u8]) -> Vec<(u64, Wire<'_>)> {
  let mut out = Vec::new();
  let mut pos = 0;
  while let Some(key) = read_varint(buf, &mut pos) {
    let value = match key & 7 {
      0 => match read_varint(buf, &mut pos) {
        Some(v) => Wire::Int(v),
        None => break,
      },
      2 => {
        let Some(len) = read_varint(buf, &mut pos) else {
          break;
        };
        let Some(bytes) = buf.get(pos..pos + len as usize) else {
          break;
        };
        pos += len as usize;
        Wire::Bytes(bytes)
      }
      1 => {
        pos += 8;
        continue;
      }
      5 => {
        pos += 4;
        continue;
      }
      _ => break,
    };
    out.push((key >> 3, value));
  }
  out
}

/// One decoded message: field lookups by number.
pub struct Msg<'a>(Vec<(u64, Wire<'a>)>);

impl<'a> Msg<'a> {
  pub fn parse(buf: &'a [u8]) -> Self {
    Self(fields(buf))
  }

  pub fn int(&self, field: u64) -> Option<u64> {
    self.0.iter().find_map(|(f, v)| match v {
      Wire::Int(i) if *f == field => Some(*i),
      _ => None,
    })
  }

  pub fn str(&self, field: u64) -> Option<String> {
    self
      .bytes(field)
      .next()
      .map(|b| String::from_utf8_lossy(b).into_owned())
  }

  pub fn msg(&self, field: u64) -> Option<Msg<'a>> {
    self.bytes(field).next().map(Msg::parse)
  }

  /// Every embedded message of a repeated field.
  pub fn msgs(&self, field: u64) -> Vec<Msg<'a>> {
    self.bytes(field).map(Msg::parse).collect()
  }

  fn bytes(&self, field: u64) -> impl Iterator<Item = &'a [u8]> + '_ {
    self.0.iter().filter_map(move |(f, v)| match v {
      Wire::Bytes(b) if *f == field => Some(*b),
      _ => None,
    })
  }
}

/// A frame from the server: its payload type and the plain payload.
pub struct Frame {
  pub kind: u64,
  pub payload: Vec<u8>,
}

pub fn decode(frame: &[u8]) -> Frame {
  let msg = Msg::parse(frame);
  let payload = msg.bytes(3).next().unwrap_or_default();
  Frame {
    kind: msg.int(1).unwrap_or(0),
    payload: plain(payload, msg.int(2).unwrap_or(0)),
  }
}

fn plain(payload: &[u8], compression: u64) -> Vec<u8> {
  match compression {
    2 => {
      let mut out = Vec::new();
      if GzDecoder::new(payload).read_to_end(&mut out).is_err() {
        out.clear();
        let _ = DeflateDecoder::new(payload).read_to_end(&mut out);
      }
      out
    }
    3 => decrypt(payload),
    _ => payload.to_vec(),
  }
}

fn decrypt(payload: &[u8]) -> Vec<u8> {
  let mut buf = payload.to_vec();
  buf.truncate(buf.len() / 16 * 16);
  let Ok(plain) = cbc::Decryptor::<Aes128>::new(AES_KEY.into(), AES_IV.into())
    .decrypt_padded_mut::<NoPadding>(&mut buf)
  else {
    return Vec::new();
  };
  let mut plain = plain.to_vec();
  let pad = plain.last().copied().unwrap_or(0) as usize;
  if (1..=16).contains(&pad)
    && pad <= plain.len()
    && plain[plain.len() - pad..]
      .iter()
      .all(|&b| b as usize == pad)
  {
    plain.truncate(plain.len() - pad);
  }
  plain
}

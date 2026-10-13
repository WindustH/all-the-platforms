//! The frame of Feishu's long connection (`pbbp2.Frame`, protobuf 2):
//!
//! ```text
//! 1 SeqID uint64, 2 LogID uint64, 3 service int32, 4 method int32
//! (0 control, 1 data), 5 headers repeated {1 key, 2 value},
//! 6 payload_encoding, 7 payload_type, 8 payload bytes, 9 LogIDNew
//! ```

use atp_core::{Error, Result};

#[derive(Debug, Clone, Default)]
pub struct Frame {
  pub seq_id: u64,
  pub log_id: u64,
  pub service: i32,
  pub method: i32,
  pub headers: Vec<(String, String)>,
  pub payload_encoding: Option<String>,
  pub payload_type: Option<String>,
  pub payload: Vec<u8>,
  pub log_id_new: Option<String>,
}

impl Frame {
  /// A control frame asking the server for a pong.
  pub fn ping(service: i32) -> Self {
    Frame {
      service,
      method: 0,
      headers: vec![("type".into(), "ping".into())],
      ..Frame::default()
    }
  }

  /// The acknowledgement of this data frame.
  pub fn ack(&self) -> Self {
    let mut ack = self.clone();
    ack.headers.push(("biz_rt".into(), "0".into()));
    ack.payload = br#"{"code":200}"#.to_vec();
    ack
  }

  pub fn header(&self, key: &str) -> Option<&str> {
    self
      .headers
      .iter()
      .find(|(k, _)| k == key)
      .map(|(_, v)| v.as_str())
  }

  pub fn encode(&self) -> Vec<u8> {
    let mut out = Vec::new();
    varint_field(&mut out, 1, self.seq_id);
    varint_field(&mut out, 2, self.log_id);
    varint_field(&mut out, 3, self.service as i64 as u64);
    varint_field(&mut out, 4, self.method as i64 as u64);
    for (k, v) in &self.headers {
      let mut h = Vec::new();
      bytes_field(&mut h, 1, k.as_bytes());
      bytes_field(&mut h, 2, v.as_bytes());
      bytes_field(&mut out, 5, &h);
    }
    if let Some(s) = &self.payload_encoding {
      bytes_field(&mut out, 6, s.as_bytes());
    }
    if let Some(s) = &self.payload_type {
      bytes_field(&mut out, 7, s.as_bytes());
    }
    if !self.payload.is_empty() {
      bytes_field(&mut out, 8, &self.payload);
    }
    if let Some(s) = &self.log_id_new {
      bytes_field(&mut out, 9, s.as_bytes());
    }
    out
  }

  pub fn decode(buf: &[u8]) -> Result<Self> {
    let mut f = Frame::default();
    let mut r = Reader { buf, pos: 0 };
    while !r.done() {
      let key = r.varint()?;
      let (field, wire) = (key >> 3, key & 7);
      match (field, wire) {
        (1, 0) => f.seq_id = r.varint()?,
        (2, 0) => f.log_id = r.varint()?,
        (3, 0) => f.service = r.varint()? as i32,
        (4, 0) => f.method = r.varint()? as i32,
        (5, 2) => {
          let mut h = Reader {
            buf: r.bytes()?,
            pos: 0,
          };
          let (mut k, mut v) = (String::new(), String::new());
          while !h.done() {
            let key = h.varint()?;
            match (key >> 3, key & 7) {
              (1, 2) => k = h.string()?,
              (2, 2) => v = h.string()?,
              (_, w) => h.skip(w)?,
            }
          }
          f.headers.push((k, v));
        }
        (6, 2) => f.payload_encoding = Some(r.string()?),
        (7, 2) => f.payload_type = Some(r.string()?),
        (8, 2) => f.payload = r.bytes()?.to_vec(),
        (9, 2) => f.log_id_new = Some(r.string()?),
        (_, w) => r.skip(w)?,
      }
    }
    Ok(f)
  }
}

fn varint(out: &mut Vec<u8>, mut v: u64) {
  while v >= 0x80 {
    out.push((v as u8) | 0x80);
    v >>= 7;
  }
  out.push(v as u8);
}

fn varint_field(out: &mut Vec<u8>, field: u64, v: u64) {
  varint(out, field << 3);
  varint(out, v);
}

fn bytes_field(out: &mut Vec<u8>, field: u64, data: &[u8]) {
  varint(out, (field << 3) | 2);
  varint(out, data.len() as u64);
  out.extend_from_slice(data);
}

struct Reader<'a> {
  buf: &'a [u8],
  pos: usize,
}

impl<'a> Reader<'a> {
  fn done(&self) -> bool {
    self.pos >= self.buf.len()
  }

  fn bad() -> Error {
    Error::upstream("malformed Feishu frame")
  }

  fn varint(&mut self) -> Result<u64> {
    let mut v = 0u64;
    for shift in (0..64).step_by(7) {
      let b = *self.buf.get(self.pos).ok_or_else(Self::bad)?;
      self.pos += 1;
      v |= u64::from(b & 0x7f) << shift;
      if b < 0x80 {
        return Ok(v);
      }
    }
    Err(Self::bad())
  }

  fn bytes(&mut self) -> Result<&'a [u8]> {
    let len = self.varint()? as usize;
    let end = self.pos.checked_add(len).ok_or_else(Self::bad)?;
    let s = self.buf.get(self.pos..end).ok_or_else(Self::bad)?;
    self.pos = end;
    Ok(s)
  }

  fn string(&mut self) -> Result<String> {
    Ok(String::from_utf8_lossy(self.bytes()?).into_owned())
  }

  fn skip(&mut self, wire: u64) -> Result<()> {
    match wire {
      0 => self.varint().map(drop),
      1 => self.advance(8),
      2 => self.bytes().map(drop),
      5 => self.advance(4),
      _ => Err(Self::bad()),
    }
  }

  fn advance(&mut self, n: usize) -> Result<()> {
    if self.pos + n > self.buf.len() {
      return Err(Self::bad());
    }
    self.pos += n;
    Ok(())
  }
}

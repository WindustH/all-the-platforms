//! MessagePack, the encoding of chat pushes, decoded into JSON values. Map
//! keys become strings (pushes key their fields by number), binary data
//! becomes text where it is UTF-8 (else base64), extension types are null.

use atp_core::{Error, Result, Value};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Map, Number};

/// Deeper nesting than this is taken for garbage.
const MAX_DEPTH: usize = 64;

/// The value in `bytes`; several values one after another come as an array.
pub fn decode(bytes: &[u8]) -> Result<Value> {
  let mut reader = Reader { bytes, pos: 0 };
  let first = reader.value(0)?;
  if reader.pos == bytes.len() {
    return Ok(first);
  }
  let mut all = vec![first];
  while reader.pos < bytes.len() {
    all.push(reader.value(0)?);
  }
  Ok(Value::Array(all))
}

struct Reader<'a> {
  bytes: &'a [u8],
  pos: usize,
}

impl Reader<'_> {
  fn take(&mut self, n: usize) -> Result<&[u8]> {
    let end = self.pos.checked_add(n).filter(|e| *e <= self.bytes.len());
    let Some(end) = end else {
      return Err(Error::upstream("truncated MessagePack data"));
    };
    let slice = &self.bytes[self.pos..end];
    self.pos = end;
    Ok(slice)
  }

  fn uint(&mut self, n: usize) -> Result<u64> {
    Ok(
      self
        .take(n)?
        .iter()
        .fold(0, |acc, b| (acc << 8) | u64::from(*b)),
    )
  }

  fn int(&mut self, n: usize) -> Result<i64> {
    let raw = self.uint(n)?;
    let shift = 64 - 8 * n as u32;
    Ok(((raw << shift) as i64) >> shift)
  }

  fn len(&mut self, n: usize) -> Result<usize> {
    Ok(self.uint(n)? as usize)
  }

  fn value(&mut self, depth: usize) -> Result<Value> {
    if depth > MAX_DEPTH {
      return Err(Error::upstream("MessagePack data nested too deep"));
    }
    let tag = self.take(1)?[0];
    match tag {
      0x00..=0x7f => Ok(Value::from(tag)),
      0x80..=0x8f => self.map(usize::from(tag & 0x0f), depth),
      0x90..=0x9f => self.array(usize::from(tag & 0x0f), depth),
      0xa0..=0xbf => self.text(usize::from(tag & 0x1f)),
      0xc0 => Ok(Value::Null),
      0xc2 => Ok(Value::Bool(false)),
      0xc3 => Ok(Value::Bool(true)),
      0xc4..=0xc6 => {
        let n = self.len(1 << (tag - 0xc4))?;
        self.binary(n)
      }
      0xc7..=0xc9 => {
        let n = self.len(1 << (tag - 0xc7))?;
        self.take(n + 1).map(|_| Value::Null)
      }
      0xca => Ok(float(f64::from(f32::from_bits(self.uint(4)? as u32)))),
      0xcb => Ok(float(f64::from_bits(self.uint(8)?))),
      0xcc..=0xcf => Ok(Value::from(self.uint(1 << (tag - 0xcc))?)),
      0xd0..=0xd3 => Ok(Value::from(self.int(1 << (tag - 0xd0))?)),
      0xd4..=0xd8 => self.take((1 << (tag - 0xd4)) + 1).map(|_| Value::Null),
      0xd9..=0xdb => {
        let n = self.len(1 << (tag - 0xd9))?;
        self.text(n)
      }
      0xdc | 0xdd => {
        let n = self.len(if tag == 0xdc { 2 } else { 4 })?;
        self.array(n, depth)
      }
      0xde | 0xdf => {
        let n = self.len(if tag == 0xde { 2 } else { 4 })?;
        self.map(n, depth)
      }
      0xe0..=0xff => Ok(Value::from(i64::from(tag as i8))),
      0xc1 => Err(Error::upstream("invalid MessagePack tag 0xc1")),
    }
  }

  fn text(&mut self, n: usize) -> Result<Value> {
    Ok(Value::String(
      String::from_utf8_lossy(self.take(n)?).into_owned(),
    ))
  }

  fn binary(&mut self, n: usize) -> Result<Value> {
    let data = self.take(n)?;
    Ok(Value::String(match std::str::from_utf8(data) {
      Ok(text) => text.to_owned(),
      Err(_) => STANDARD.encode(data),
    }))
  }

  fn array(&mut self, n: usize, depth: usize) -> Result<Value> {
    let mut items = Vec::with_capacity(n.min(1024));
    for _ in 0..n {
      items.push(self.value(depth + 1)?);
    }
    Ok(Value::Array(items))
  }

  fn map(&mut self, n: usize, depth: usize) -> Result<Value> {
    let mut map = Map::new();
    for _ in 0..n {
      let key = match self.value(depth + 1)? {
        Value::String(s) => s,
        other => other.to_string(),
      };
      map.insert(key, self.value(depth + 1)?);
    }
    Ok(Value::Object(map))
  }
}

fn float(f: f64) -> Value {
  Number::from_f64(f).map_or(Value::Null, Value::Number)
}

//! Protobuf by hand, for platforms that read a few fields of binary frames
//! (live rooms, push connections) without their schemas: a lazy reader
//! ([`Msg`]) and writers for varint and length-delimited fields.

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

/// A varint field.
pub fn put_int(out: &mut Vec<u8>, number: u32, value: u64) {
  put_varint(out, u64::from(number << 3));
  put_varint(out, value);
}

/// A length-delimited field (bytes, a string or an embedded message).
pub fn put_bytes(out: &mut Vec<u8>, number: u32, bytes: &[u8]) {
  put_varint(out, u64::from(number << 3 | 2));
  put_varint(out, bytes.len() as u64);
  out.extend_from_slice(bytes);
}

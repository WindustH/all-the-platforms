//! The generator's `encodeUtil`: field framing, varints, checksums and its
//! base64 variant. Each function ports one program of the bytecode VM in
//! chunk 3636 (`ec_sc`, `ec_nc`, `ec_va`, `ec_ek`, `ec_pbc`, `ec_budget`,
//! `ec_encode`), keeping JavaScript's number quirks where they matter.

/// The default alphabet of `ec_encode`; `=` follows it as index 64.
pub const ALPHABET: &[u8; 64] = b"9240gsB6PftGXnlQTw_pdvz7EekDmuAWCVZ5UF-MSK1IHOchoaxqYyj8Jb3LrNiR";

/// What a field carries after its tag.
pub enum Value<'a> {
  /// Nothing: the tag alone.
  None,
  /// A string, as [`text`] bytes.
  Text(&'a str),
  /// A number, as [`number`] bytes.
  Number(u64),
}

/// `ec_sc`: UTF-8 bytes of the first 255 UTF-16 units. Characters outside
/// the BMP are dropped (`ec_charCode` skips surrogates).
pub fn text(text: &str) -> Vec<u8> {
  let mut out = Vec::with_capacity(text.len());
  let mut units = 0;
  for c in text.chars() {
    units += c.len_utf16();
    if units > 255 {
      break;
    }
    if (c as u32) <= 0xFFFF {
      let mut buf = [0; 4];
      out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
    }
  }
  out
}

/// `ec_nc`: big-endian bytes without leading zeros (`[0]` for zero).
pub fn number(n: u64) -> Vec<u8> {
  let bytes = n.to_be_bytes();
  let skip = bytes.iter().take_while(|&&b| b == 0).count().min(7);
  bytes[skip..].to_vec()
}

/// `ec_va`: 7-bit groups, least significant first, the high bit set while
/// more follow. The groups come from the binary string of `n`, the stop
/// test from its low 32 bits (`n & -128`, then `n >>> 7`), as in the page.
pub fn varint(n: u64) -> Vec<u8> {
  let bits = format!("{n:b}");
  let width = bits.len().div_ceil(7) * 7;
  let bits = format!("{bits:0>width$}");
  let mut out = Vec::new();
  let mut low = n as u32;
  let mut end = bits.len() as i64;
  while end >= 0 {
    let start = (end - 7).max(0) as usize;
    let group = u8::from_str_radix(&bits[start..end as usize], 2).unwrap_or(0);
    if (low as i32) & -128 == 0 {
      out.push(group);
      break;
    }
    out.push(group | if end > 0 { 0x80 } else { 1 });
    low >>= 7;
    end -= 7;
  }
  out
}

/// `ec_ek`: a tag byte (`kind << 3 | length` when the payload has 1 to 7
/// bytes, else `kind << 3`), a varint length when longer, then the payload.
pub fn field(kind: u8, value: Value) -> Vec<u8> {
  let (payload, len) = match value {
    Value::None => (Vec::new(), 0),
    Value::Text(t) => {
      let bytes = text(t);
      let len = bytes.len();
      (bytes, len)
    }
    Value::Number(n) => {
      let bytes = number(n);
      let len = bytes.len();
      (bytes, len)
    }
  };
  let low = if (1..=7).contains(&len) { len as u8 } else { 0 };
  let mut out = vec![(kind << 3) | low];
  if len > 7 {
    out.extend(varint(len as u64));
  }
  out.extend(payload);
  out
}

/// `ec_pbc`: the CRC-32 of `text` without whitespace, big-endian.
pub fn checksum(text: &str) -> [u8; 4] {
  let stripped: String = text.chars().filter(|&c| !js_space(c)).collect();
  let mut crc = flate2::Crc::new();
  crc.update(stripped.as_bytes());
  crc.sum().to_be_bytes()
}

/// JavaScript's `\s`.
fn js_space(c: char) -> bool {
  matches!(
    c,
    '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
      ..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}'
  )
}

/// `ec_budget`: the index shuffle `0as` tokens use, keyed by `offset`.
fn budget(index: usize, offset: usize) -> usize {
  match index {
    64 => 64,
    63 => offset,
    i if i >= offset => i + 1,
    i => i,
  }
}

/// `ec_encode`: base64 over [`ALPHABET`] without padding. With an `offset`
/// (0 to 63) every index goes through [`budget`] and the offset's letter
/// is appended.
pub fn encode(data: &[u8], offset: Option<usize>) -> String {
  let letter = |i: usize| -> Option<char> {
    let i = offset.map_or(i, |o| budget(i, o));
    ALPHABET.get(i).map(|&b| b as char)
  };
  let mut out = String::with_capacity(data.len() * 4 / 3 + 2);
  for chunk in data.chunks(3) {
    let (b0, b1, b2) = (chunk[0] as usize, chunk.get(1), chunk.get(2));
    let (c1, c2) = (b1.map_or(0, |&b| b as usize), b2.map_or(0, |&b| b as usize));
    let mut indexes = [
      b0 >> 2,
      (b0 & 3) << 4 | c1 >> 4,
      (c1 & 15) << 2 | c2 >> 6,
      c2 & 63,
    ];
    if b1.is_none() {
      indexes[2] = 64;
      indexes[3] = 64;
    } else if b2.is_none() {
      indexes[3] = 64;
    }
    out.extend(indexes.iter().filter_map(|&i| letter(i)));
  }
  if let Some(o) = offset {
    out.push(ALPHABET[o] as char);
  }
  out
}

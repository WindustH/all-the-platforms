//! The book ids in WeRead's links. A reader or detail link
//! (`/web/reader/<hash>`, `/web/bookDetail/<hash>`) carries a hash of the
//! book id, the web pages' `_e(id)`: three hex digits of the id's MD5, a
//! type digit (`3` numeric, `4` other), `2`, the MD5's last two digits, then
//! the id itself as hex chunks (nine decimal digits each, or the character
//! codes) behind a two-digit length, joined by `g`, padded to twenty
//! characters, and three digits of the MD5 of all that.

use md5::{Digest, Md5};

fn md5_hex(text: &str) -> String {
  format!("{:x}", Md5::digest(text.as_bytes()))
}

fn numeric(id: &str) -> bool {
  !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit())
}

/// The id as the type digit and its hex chunks.
fn chunks(id: &str) -> (char, Vec<String>) {
  if numeric(id) {
    let parts = id
      .as_bytes()
      .chunks(9)
      .map(|c| {
        let n: u64 = std::str::from_utf8(c)
          .ok()
          .and_then(|s| s.parse().ok())
          .unwrap_or(0);
        format!("{n:x}")
      })
      .collect();
    return ('3', parts);
  }
  let hex = id.chars().map(|c| format!("{:x}", c as u32)).collect();
  ('4', vec![hex])
}

/// The hash of a book id that links use.
pub fn encode(id: &str) -> String {
  let digest = md5_hex(id);
  let (kind, parts) = chunks(id);
  let mut out = format!("{}{kind}2{}", &digest[..3], &digest[digest.len() - 2..]);
  let joined: Vec<String> = parts
    .iter()
    .map(|part| format!("{:02x}{part}", part.len()))
    .collect();
  out.push_str(&joined.join("g"));
  if out.len() < 20 {
    let missing = 20 - out.len();
    out.push_str(&digest[..missing]);
  }
  let check = md5_hex(&out);
  out.push_str(&check[..3]);
  out
}

/// The hex chunks of a hash, read by their lengths.
fn split(hash: &str) -> Option<(char, Vec<&str>)> {
  let kind = hash.chars().nth(3)?;
  let mut rest = hash.get(7..)?;
  let mut parts = Vec::new();
  loop {
    let len = usize::from_str_radix(rest.get(..2)?, 16).ok()?;
    parts.push(rest.get(2..2 + len)?);
    rest = &rest[2 + len..];
    match rest.strip_prefix('g') {
      Some(next) => rest = next,
      None => return Some((kind, parts)),
    }
  }
}

/// The book id behind a hash, checked by hashing it again.
pub fn decode(hash: &str) -> Option<String> {
  let hash = hash.to_ascii_lowercase();
  let (kind, parts) = split(&hash)?;
  let candidates: Vec<String> = match kind {
    '3' => numeric_ids(&parts)?,
    '4' => vec![text_id(parts.first()?)?],
    _ => return None,
  };
  candidates.into_iter().find(|id| encode(id) == hash)
}

/// A numeric id: full nine-digit chunks, and a last chunk whose leading
/// zeros the hash lost (each width is tried).
fn numeric_ids(parts: &[&str]) -> Option<Vec<String>> {
  let (last, head) = parts.split_last()?;
  let mut prefix = String::new();
  for part in head {
    prefix.push_str(&format!("{:09}", u64::from_str_radix(part, 16).ok()?));
  }
  let tail = u64::from_str_radix(last, 16).ok()?;
  Some(
    (1..=9)
      .map(|width| format!("{prefix}{tail:0width$}"))
      .collect(),
  )
}

/// A text id (`MP_WXS_…`, `CB_…`): two hex digits per ASCII character.
fn text_id(hex: &str) -> Option<String> {
  let bytes = hex.as_bytes();
  bytes
    .chunks(2)
    .map(|pair| {
      let s = std::str::from_utf8(pair).ok()?;
      u8::from_str_radix(s, 16).ok().map(char::from)
    })
    .collect()
}

//! TikTok's SlardarWAF "Please wait..." page: a proof of work the browser
//! solves before it is served the real page. The challenge (`#cs`) holds a
//! prefix `a` and the SHA-256 `c` of `a` followed by a small decimal number;
//! the answer goes back as cookies and the page is loaded again, as the
//! challenge script does.

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};
use sha2::{Digest, Sha256};
use social_core::{Value, ValueExt};

/// Highest number the challenge script tries.
const LIMIT: u32 = 1_000_000;

/// Whether a page body is the challenge stub instead of content.
pub fn is_challenge(html: &str) -> bool {
  html.len() < 20_000 && (html.contains("_wafchallengeid") || html.contains("SlardarWAF"))
}

/// The cookies that answer the challenge in `html`, as `(name, value)`.
pub fn solve(html: &str) -> Option<Vec<(String, String)>> {
  let cs = class_of(html, "cs")?;
  let mut challenge: Value = serde_json::from_slice(&decode(&cs)?).ok()?;
  let prefix = decode(&challenge.str("v.a")?)?;
  let expect = decode(&challenge.str("v.c")?)?;
  let n = (0..=LIMIT).find(|n| {
    let mut h = Sha256::new();
    h.update(&prefix);
    h.update(n.to_string());
    h.finalize().as_slice() == expect.as_slice()
  })?;
  challenge["d"] = STANDARD.encode(n.to_string()).into();
  let mut cookies = vec![(
    class_of(html, "wci").unwrap_or_else(|| "_wafchallengeid".into()),
    STANDARD.encode(challenge.to_string()),
  )];
  if let Some(rs) = class_of(html, "rs").filter(|v| !v.is_empty()) {
    let name = class_of(html, "rci").unwrap_or_else(|| "waforiginalreid".into());
    cookies.push((name, rs));
  }
  if let Some(id) = class_of(html, "rs_id").filter(|v| !v.is_empty()) {
    cookies.push(("waforigin_id".into(), id));
  }
  Some(cookies)
}

/// `class` of the element with `id`.
fn class_of(html: &str, id: &str) -> Option<String> {
  let at = html.find(&format!("id=\"{id}\""))?;
  let rest = &html[at..html[at..].find('>').map_or(html.len(), |e| at + e)];
  let start = rest.find("class=\"")? + 7;
  let end = rest[start..].find('"')?;
  Some(rest[start..start + end].to_owned())
}

fn decode(s: &str) -> Option<Vec<u8>> {
  let s = s.trim();
  STANDARD
    .decode(s)
    .or_else(|_| STANDARD_NO_PAD.decode(s.trim_end_matches('=')))
    .ok()
}

//! AWS Signature Version 4 as ByteDance's upload gateways take it
//! (ImageX and VOD on `*.bytedanceapi.com`, with temporary STS keys).
//!
//! The web upload SDK signs only the headers it sets itself: `x-amz-date`,
//! `x-amz-security-token` and, on requests with a body,
//! `x-amz-content-sha256`; the canonical query is sorted by name and
//! RFC 3986-encoded. Callers pass the headers to sign, so the same code
//! reproduces AWS's published test vectors.

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

/// What a request is signed for.
pub struct Scope<'a> {
  pub region: &'a str,
  pub service: &'a str,
}

/// The parts of a request the signature covers.
pub struct Request<'a> {
  pub method: &'a str,
  pub path: &'a str,
  pub query: &'a [(String, String)],
  /// Headers to sign; names in any case.
  pub headers: &'a [(String, String)],
  /// Hex SHA-256 of the body ([`sha256_hex`] of an empty body for GET).
  pub payload_hash: &'a str,
}

/// Temporary credentials.
pub struct Keys<'a> {
  pub access_key: &'a str,
  pub secret: &'a str,
}

/// `20150830T123600Z`.
pub fn amz_date(ts: jiff::Timestamp) -> String {
  ts.strftime("%Y%m%dT%H%M%SZ").to_string()
}

pub fn sha256_hex(data: &[u8]) -> String {
  hex(&Sha256::digest(data))
}

/// The `Authorization` header of `req` signed at `date` (`amz_date`).
pub fn authorization(req: &Request, keys: &Keys, scope: &Scope, date: &str) -> String {
  let day = &date[..8.min(date.len())];
  let credential = format!("{day}/{}/{}/aws4_request", scope.region, scope.service);
  let (canonical, signed) = canonical_request(req);
  let to_sign = format!(
    "AWS4-HMAC-SHA256\n{date}\n{credential}\n{}",
    sha256_hex(canonical.as_bytes())
  );
  let mut key = hmac(format!("AWS4{}", keys.secret).as_bytes(), day.as_bytes());
  for part in [scope.region, scope.service, "aws4_request"] {
    key = hmac(&key, part.as_bytes());
  }
  let signature = hex(&hmac(&key, to_sign.as_bytes()));
  format!(
    "AWS4-HMAC-SHA256 Credential={}/{credential}, SignedHeaders={signed}, Signature={signature}",
    keys.access_key
  )
}

/// The canonical request and the signed header names.
fn canonical_request(req: &Request) -> (String, String) {
  let mut headers: Vec<(String, String)> = req
    .headers
    .iter()
    .map(|(k, v)| (k.to_ascii_lowercase(), collapse(v)))
    .collect();
  headers.sort();
  let lines: String = headers.iter().map(|(k, v)| format!("{k}:{v}\n")).collect();
  let signed = headers
    .iter()
    .map(|(k, _)| k.as_str())
    .collect::<Vec<_>>()
    .join(";");
  let canonical = format!(
    "{}\n{}\n{}\n{lines}\n{signed}\n{}",
    req.method,
    req.path,
    canonical_query(req.query),
    req.payload_hash
  );
  (canonical, signed)
}

/// Pairs sorted by name (then value), both RFC 3986-encoded.
pub fn canonical_query(pairs: &[(String, String)]) -> String {
  let mut encoded: Vec<(String, String)> =
    pairs.iter().map(|(k, v)| (encode(k), encode(v))).collect();
  encoded.sort();
  encoded
    .iter()
    .map(|(k, v)| format!("{k}={v}"))
    .collect::<Vec<_>>()
    .join("&")
}

/// Percent-encoding of everything but `A-Z a-z 0-9 - _ . ~`.
pub fn encode(s: &str) -> String {
  let mut out = String::with_capacity(s.len());
  for b in s.bytes() {
    if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
      out.push(char::from(b));
    } else {
      out.push_str(&format!("%{b:02X}"));
    }
  }
  out
}

fn collapse(value: &str) -> String {
  value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
  let mut mac = <Hmac<Sha256>>::new_from_slice(key).expect("HMAC takes any key");
  mac.update(data);
  mac.finalize().into_bytes().to_vec()
}

fn hex(bytes: &[u8]) -> String {
  bytes.iter().map(|b| format!("{b:02x}")).collect()
}

//! `a_bogus`: the query signature of ByteDance's web SDK `bdms.js`
//! (1.0.1.19-fix.01), which Douyin and Toutiao both load with their own
//! page and app ids,
//! ported from the reverse-engineered implementation in
//! `Evil0ctal/Douyin_TikTok_Download_API` (`src/dtk/signing/native/abogus.py`,
//! Apache-2.0) as `JoeanAmier/TikTokDownloader` adapts it.
//!
//! Fifty scalar fields (clock, environment probes, page and app ids, three
//! bytes from each of the query / body / User-Agent digests) plus the browser
//! geometry are masked with noise, sealed with the SDK's RC4 variant and
//! encoded with its base64 alphabet. The User-Agent must be the one the
//! request is sent with: one digest chain is bound to it.

use rand::Rng;
use sm3::{Digest, Sm3};

/// `s3` encodes the User-Agent of the third chain, `s4` the signature.
const S3: &[u8; 64] = b"ckdp1h4ZKsUB80/Mfvw36XIgR25+WQAlEi7NLboqYTOPuzmFjJnryx9HVGDaStCe";
const S4: &[u8; 64] = b"Dkdpgh2ZmsQB80/MfvV36XI1R45-WUAlEixNLwoqYTOPuzKFjJnry79HbGcaStCe";

/// Appended to the query and the body before hashing.
const SALT: &str = "dhzx";
/// Two plain header bytes (a format magic).
const HEADER_MAGIC: [u8; 2] = [3, 82];
/// "1.0.1.19-fix.01" as version blocks.
const SDK_VERSION: [u8; 4] = [1, 0, 1, 0];
/// Single-byte key of the payload cipher.
const PAYLOAD_KEY: u8 = 0xD3;
/// 2024-07-24T16:00:00Z; one field counts fortnights since.
const FORTNIGHT_EPOCH_MS: i64 = 1_721_836_800_000;
/// The page and app ids a site initializes the SDK with; the query of a
/// signed request carries the same `aid`.
#[derive(Debug, Clone, Copy)]
pub struct App {
  pub aid: u64,
  pub page_id: u64,
}

pub const DOUYIN: App = App {
  aid: 6383,
  page_id: 6241,
};
pub const TOUTIAO: App = App {
  aid: 24,
  page_id: 6457,
};

const NOISE_MASKS: [u8; 3] = [0x91, 0x42, 0x2C];
const DATA_MASKS: [u8; 3] = [0x6E, 0xBD, 0xD3];

/// Order of the fifty scalar fields in the payload.
const FIELD_ORDER: [u8; 50] = [
  34, 44, 56, 61, 73, 29, 70, 45, 35, 49, 38, 66, 51, 68, 28, 48, 64, 47, 30, 71, 26, 55, 31, 69,
  59, 40, 62, 63, 27, 72, 41, 74, 57, 52, 42, 39, 33, 67, 53, 43, 65, 46, 36, 24, 60, 32, 79, 80,
  84, 85,
];

/// One hash chain: the three fields it fills, the two digest indices written
/// directly, and `(offset, sentinel, fallback)` of its canary byte.
struct Chain {
  slots: [u8; 3],
  indices: [usize; 2],
  canary: (usize, u8, u8),
}

const QUERY_CHAIN: Chain = Chain {
  slots: [48, 49, 51],
  indices: [9, 18],
  canary: (3, 11, 12),
};
const BODY_CHAIN: Chain = Chain {
  slots: [52, 53, 55],
  indices: [10, 19],
  canary: (4, 8, 9),
};
const UA_CHAIN: Chain = Chain {
  slots: [56, 57, 59],
  indices: [11, 21],
  canary: (5, 12, 13),
};

/// Untouched browser environment reports.
const ENV_FLAGS: u64 = 1;
const DETECT_FLAGS: u64 = 14;
const NR_FLAGS: u64 = 0x21;
const NR_TAG: [u8; 4] = [0; 4];
const TRIPWIRE_LOCKED: u8 = 3;
/// Fewer than 140 signatures on this page.
const CALL_BUCKET: u8 = 6;
const TRIPWIRE_SET: u8 = 0xB2;
const TRIPWIRE_FREE: u8 = 0x4D;

/// Inner and outer window, available area and screen of a 1536×864 desktop,
/// and the platform; the query's `screen_*` / `browser_platform` match it.
pub const BROWSER_INFO: &str = "1536|742|1536|864|1536|864|1536|864|MacIntel";

/// The `a_bogus` of one request (not percent-encoded). `body` is the exact
/// body sent (empty for GET and multipart uploads).
pub fn sign(app: App, query: &str, body: &str, user_agent: &str, now_ms: i64) -> String {
  let mut rng = rand::rng();
  let fields = fields(app, query, body, user_agent, now_ms);

  let mut version = mask_pair([SDK_VERSION[0], SDK_VERSION[1]], &mut rng, None, None);
  let probe = probe_noise(&mut rng);
  let tripwire = tripwire_noise(&mut rng);
  version.extend(mask_pair(
    [SDK_VERSION[2], SDK_VERSION[3]],
    &mut rng,
    Some(probe),
    Some(tripwire),
  ));

  let mut checksum = version.iter().fold(0u8, |c, b| c ^ b);
  let mut body_bytes: Vec<u8> = Vec::with_capacity(100);
  for name in FIELD_ORDER {
    let value = fields[usize::from(name)];
    checksum ^= value;
    body_bytes.push(value);
  }
  body_bytes.extend(js_bytes(BROWSER_INFO));
  body_bytes.extend(js_bytes(&format!("{},", (now_ms + 3) & 0xFF)));
  body_bytes.push(checksum);

  let noise = header_noise(user_agent, &mut rng);
  let mut out = mask_pair(HEADER_MAGIC, &mut rng, None, Some(noise));
  let mut sealed = version;
  sealed.extend(expand_noise(&body_bytes, &mut rng));
  out.extend(rc4(&[PAYLOAD_KEY], &sealed));
  base64(&out, S4)
}

/// The fifty scalars, indexed by their field number (`L24` → 24).
fn fields(app: App, query: &str, body: &str, user_agent: &str, now_ms: i64) -> [u8; 86] {
  let mut f = [0u8; 86];
  let now = now_ms as u64;
  let ink = now.saturating_sub(1);
  let fortnights = (now_ms - FORTNIGHT_EPOCH_MS).max(0) / (1000 * 60 * 60 * 24 * 14);
  let info_len = js_bytes(BROWSER_INFO).len();
  let tail_len = js_bytes(&format!("{},", (now_ms + 3) & 0xFF)).len();

  f[24] = 41;
  f[26] = fortnights as u8;
  f[27] = CALL_BUCKET;
  f[28] = 3;
  f[35] = (ENV_FLAGS & 0xFF) as u8;
  f[36] = ((ENV_FLAGS / 256) & 0xFF) as u8;
  f[38] = (NR_FLAGS & 0xFF) as u8;
  f[39] = ((NR_FLAGS >> 8) & 0xFF) as u8;
  f[66] = TRIPWIRE_LOCKED;
  f[79] = (info_len & 0xFF) as u8;
  f[80] = ((info_len >> 8) & 0xFF) as u8;
  f[84] = (tail_len & 0xFF) as u8;
  f[85] = ((tail_len >> 8) & 0xFF) as u8;
  put_le(&mut f, 29, now, 6);
  put_le(&mut f, 44, DETECT_FLAGS, 4);
  f[40..44].copy_from_slice(&NR_TAG);
  put_le(&mut f, 60, ink, 6);
  put_le(&mut f, 67, app.page_id, 4);
  put_le(&mut f, 71, app.aid, 4);

  for (chain, digest) in [
    (&QUERY_CHAIN, digest_of(query)),
    (&BODY_CHAIN, digest_of(body)),
    (&UA_CHAIN, user_agent_digest(user_agent)),
  ] {
    let (offset, sentinel, fallback) = chain.canary;
    let canary = digest[offset..]
      .iter()
      .copied()
      .find(|b| *b != sentinel)
      .unwrap_or(fallback);
    let bytes = [digest[chain.indices[0]], digest[chain.indices[1]], canary];
    for (slot, value) in chain.slots.iter().zip(bytes) {
      f[usize::from(*slot)] = value;
    }
  }
  f
}

fn put_le(f: &mut [u8; 86], start: usize, value: u64, count: usize) {
  for i in 0..count {
    f[start + i] = ((value >> (8 * i)) & 0xFF) as u8;
  }
}

fn sm3(data: &[u8]) -> [u8; 32] {
  Sm3::digest(data).into()
}

/// `SM3(SM3(text + SALT))`.
fn digest_of(text: &str) -> [u8; 32] {
  sm3(&sm3(format!("{text}{SALT}").as_bytes()))
}

/// The third chain: the User-Agent sealed with RC4 (key from the environment
/// probes), base64 with the `s3` alphabet, then one SM3.
fn user_agent_digest(user_agent: &str) -> [u8; 32] {
  let key = [
    (ENV_FLAGS / 256) as u8,
    (ENV_FLAGS % 256) as u8,
    (DETECT_FLAGS % 256) as u8,
  ];
  let sealed = rc4(&key, &js_bytes(user_agent.trim()));
  sm3(base64(&sealed, S3).as_bytes())
}

/// Bytes the SDK reads with `charCodeAt`: one byte below U+0100, otherwise
/// the two big-endian bytes of the UTF-16 unit.
fn js_bytes(text: &str) -> Vec<u8> {
  let mut out = Vec::with_capacity(text.len());
  for unit in text.encode_utf16() {
    if unit & 0xFF00 != 0 {
      out.push((unit >> 8) as u8);
    }
    out.push((unit & 0xFF) as u8);
  }
  out
}

/// The SDK's RC4: descending initial box and a multiplicative key schedule.
fn rc4(key: &[u8], data: &[u8]) -> Vec<u8> {
  let mut sbox = [0u8; 256];
  for (i, slot) in sbox.iter_mut().enumerate() {
    *slot = (255 - i) as u8;
  }
  let mut j: usize = 0;
  for i in 0..256 {
    j = (j * usize::from(sbox[i]) + j + usize::from(key[i % key.len()])) % 256;
    sbox.swap(i, j);
  }
  let (mut i, mut j) = (0usize, 0usize);
  data
    .iter()
    .map(|byte| {
      i = (i + 1) % 256;
      j = (j + usize::from(sbox[i])) % 256;
      sbox.swap(i, j);
      byte ^ sbox[(usize::from(sbox[i]) + usize::from(sbox[j])) % 256]
    })
    .collect()
}

/// Base64 over an SDK alphabet, padded with literal `=`.
fn base64(data: &[u8], table: &[u8; 64]) -> String {
  let mut out = String::with_capacity(data.len() * 4 / 3 + 4);
  for chunk in data.chunks(3) {
    let mut block = [0u8; 3];
    block[..chunk.len()].copy_from_slice(chunk);
    let n = (u32::from(block[0]) << 16) | (u32::from(block[1]) << 8) | u32::from(block[2]);
    for k in 0..=chunk.len() {
      out.push(char::from(table[((n >> (18 - 6 * k)) & 0x3F) as usize]));
    }
  }
  while !out.len().is_multiple_of(4) {
    out.push('=');
  }
  out
}

fn rand_below(rng: &mut impl Rng, n: f64) -> u32 {
  (rng.random::<f64>() * n) as u32
}

/// Two payload bytes spread over four, the other half noise (or reports).
fn mask_pair(pair: [u8; 2], rng: &mut impl Rng, low: Option<u8>, high: Option<u8>) -> Vec<u8> {
  let noise = rand_below(rng, 65535.0);
  let low = low.unwrap_or((noise & 0xFF) as u8);
  let high = high.unwrap_or(((noise >> 8) & 0xFF) as u8);
  vec![
    (low & 0xAA) | (pair[0] & 0x55),
    (low & 0x55) | (pair[0] & 0xAA),
    (high & 0xAA) | (pair[1] & 0x55),
    (high & 0x55) | (pair[1] & 0xAA),
  ]
}

/// Three data bytes and one noise byte become four carrier bytes.
fn expand_noise(body: &[u8], rng: &mut impl Rng) -> Vec<u8> {
  let mut out = Vec::with_capacity(body.len() * 4 / 3 + 2);
  for group in body.chunks(3) {
    if group.len() < 3 {
      out.push(group[0]);
      if group.len() > 1 && group[1] != 0 {
        out.push(group[1]);
      }
      continue;
    }
    let noise = (rand_below(rng, 1000.0) & 0xFF) as u8;
    for k in 0..3 {
      out.push((noise & NOISE_MASKS[k]) | (group[k] & DATA_MASKS[k]));
    }
    out.push(
      (group[0] & NOISE_MASKS[0]) | (group[1] & NOISE_MASKS[1]) | (group[2] & NOISE_MASKS[2]),
    );
  }
  out
}

/// The browser family, reported as noise in the header (Chrome: 0..40).
fn header_noise(user_agent: &str, rng: &mut impl Rng) -> u8 {
  let ua = user_agent.to_lowercase();
  let base = [
    ("edg", 125),
    ("huawei", 170),
    ("firefox", 40),
    ("chrome", 0),
    ("safari", 81),
  ]
  .iter()
  .find(|(name, _)| ua.contains(name))
  .map_or(210, |(_, base)| *base);
  ((base + rand_below(rng, 40.0)) & 0xFF) as u8
}

/// Above 109 the probe is forced odd.
fn probe_noise(rng: &mut impl Rng) -> u8 {
  let value = rand_below(rng, 240.0);
  let value = if value > 109 {
    value + value % 2 + 1
  } else {
    value
  };
  (value & 0xFF) as u8
}

/// "All tripwires present and locked", with real noise.
fn tripwire_noise(rng: &mut impl Rng) -> u8 {
  ((rand_below(rng, 255.0) as u8) & TRIPWIRE_FREE) | TRIPWIRE_SET
}

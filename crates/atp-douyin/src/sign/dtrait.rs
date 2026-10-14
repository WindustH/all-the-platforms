//! `x-tt-session-dtrait`: the device traits Douyin's risk control wants on
//! writes. A blob of browser features (MurmurHash3 of each, bit flags) is
//! sealed with AES-128-CBC under a fresh key, and that key with the RSA
//! key built into the login page bundle (`d0`).
//!
//! Layout and the captured Chrome profile from `cv-cat/catbus`
//! (`src/platforms/douyin/web/dtrait.ts`, `crypto.ts`,
//! `static/douyin/dtrait_profile.json`, MIT). Features that need real
//! rendering (canvas, WebGL, audio, fonts) are that capture's hashes; the
//! browser facts ATP states elsewhere (User-Agent, platform, cores, memory,
//! screen) replace the captured ones.

use aes::Aes128;
use atp_core::http::USER_AGENT;
use atp_core::{Error, Result, json};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use cbc::cipher::block_padding::Pkcs7;
use cbc::cipher::{BlockEncryptMut, KeyIvInit};
use rand::Rng;
use rsa::pkcs1::DecodeRsaPublicKey;
use rsa::rand_core::OsRng;
use rsa::{Pkcs1v15Encrypt, RsaPublicKey};

/// The `d0` key of the login page bundle.
const PAGE_KEY: &str = "-----BEGIN RSA PUBLIC KEY-----
MIIBCgKCAQEA4+dvvY7uM+opc+nLG/DumSeFo7aVcIm1LO+mUIrWprRzP0a1GpEQ
4qtO9MRf/lwEIx38+4BZ4XOGzevVtuUvfIOUE7ATtQW7FKJf5PnSLCI6/k0vl1FC
LUSVmEP6qPJrIjZ4zXoqi3yuNZ+1odbRA//GHgcgSyyyaLqzwjkpWGXouMYmvYsS
pbk/fv2EWABstPNxWa4EOBCadTUPkXNQ76NBEP9xzfJS20wiDv1oS/vKvrSUpWcJ
nazkkBvqQbbAqVbQFTF/DPikppu6ZT6lGIXvJKCreeFiHA2qK6tS18SWVHW9ARz1
+pjB1eqIIYtohWpT2B40OC4O8tVeBAnbiQIDAQAB
-----END RSA PUBLIC KEY-----";
const KEY_VERSION: &str = "d0";
const SDK_VERSION: &str = "1.0.0.16";

/// Header byte: `dTraitType` 1, access type 0, version 0.
const HEAD: u8 = 0x20;
/// The ten boolean features (1–10) as the SDK packs them: 4, 6 and 7 set.
const BOOLS: [u8; 5] = [0, 0, 0, 0, 0b1101_0000];

/// MurmurHash3 of the rendered features of the captured Chrome (by number).
const RENDERED: [(u8, u32); 19] = [
  (1, 755_324_578),
  (2, 3_795_871_487),
  (3, 3_493_564_185),
  (4, 3_297_227_221),
  (5, 1_383_342_405),
  (6, 762_554_969),
  (7, 2_246_935_287),
  (8, 2_868_190_146),
  (9, 2_046_748_658),
  (10, 500_342_467),
  (13, 1_189_848_898),
  (20, 2_347_868_216),
  (21, 2_719_920_782),
  (22, 308_954_736),
  (23, 2_240_922_934),
  (24, 466_459_917),
  (25, 1_482_679_892),
  (26, 3_117_199_483),
  (33, 1_555_653_270),
];

/// The header for a write to `path`.
pub fn header(path: &str) -> Result<String> {
  let mut rng = rand::rng();
  let key: [u8; 16] = rng.random();
  let iv: [u8; 16] = rng.random();
  let key_hex: String = key.iter().map(|b| format!("{b:02x}")).collect();
  let public = RsaPublicKey::from_pkcs1_pem(PAGE_KEY)
    .map_err(|e| Error::internal(format!("dtrait key: {e}")))?;
  let sealed_key = public
    .encrypt(&mut OsRng, Pkcs1v15Encrypt, key_hex.as_bytes())
    .map_err(|e| Error::internal(format!("dtrait key: {e}")))?;
  let payload = json!({
    "dtrait": blob(),
    "timestamp": jiff::Timestamp::now().as_second(),
    "sdkVersion": SDK_VERSION,
    "path": path,
  })
  .to_string();
  let mut buf = payload.as_bytes().to_vec();
  buf.resize(payload.len() + 16, 0);
  let len = cbc::Encryptor::<Aes128>::new(&key.into(), &iv.into())
    .encrypt_padded_mut::<Pkcs7>(&mut buf, payload.len())
    .expect("room for the padding")
    .len();
  buf.truncate(len);
  let mut sealed = iv.to_vec();
  sealed.extend(buf);
  Ok(format!(
    "{KEY_VERSION}_{}_{}",
    STANDARD.encode(sealed_key),
    STANDARD.encode(sealed)
  ))
}

/// The inner blob: head byte, boolean features, then 34 string features as
/// `[tag][murmur3 big-endian]`.
fn blob() -> String {
  let computed = computed();
  let mut out = vec![HEAD];
  out.extend(BOOLS);
  for n in 1..=34u8 {
    let hash = RENDERED
      .iter()
      .find(|(i, _)| *i == n)
      .map(|(_, h)| *h)
      .or_else(|| {
        computed
          .iter()
          .find(|(i, _)| *i == n)
          .map(|(_, s)| murmur3(s))
      })
      .unwrap_or_default();
    out.push(if n <= 33 { 31 + n } else { 71 });
    out.extend(hash.to_be_bytes());
  }
  STANDARD.encode(out)
}

/// Features computed from browser facts, by number. 11 and 12 are V8's
/// results of fixed `Math` calls.
fn computed() -> Vec<(u8, String)> {
  vec![
    (
      11,
      "-1.4214488238747245,0.5493061443340549,0.5493061443340549,-0.8390715290095377".into(),
    ),
    (
      12,
      "1.7182818284590453,1.9275814160560206e-50,0.8178819121159085,-1.4214488238747245".into(),
    ),
    (14, "10,4g".into()),
    (15, "zh-CN,zh-CN,zh,en".into()),
    (16, ",Google Inc.".into()),
    (17, "MacIntel,undefined".into()),
    (18, String::new()),
    (19, USER_AGENT.into()),
    (27, "zh-CN+Asia/Shanghai".into()),
    (28, "default".into()),
    (29, "10,8,8,0".into()),
    (30, "839,0,25,1536".into()),
    (31, "864,1536".into()),
    (32, "24,24,2".into()),
    (34, "1".into()),
  ]
}

/// MurmurHash3 (x86, 32-bit, seed 0) of the UTF-8 bytes.
fn murmur3(text: &str) -> u32 {
  const C1: u32 = 0xcc9e_2d51;
  const C2: u32 = 0x1b87_3593;
  let data = text.as_bytes();
  let mut h: u32 = 0;
  let (blocks, tail) = data.as_chunks::<4>();
  for block in blocks {
    let k = u32::from_le_bytes(*block);
    h ^= k.wrapping_mul(C1).rotate_left(15).wrapping_mul(C2);
    h = h.rotate_left(13).wrapping_mul(5).wrapping_add(0xe654_6b64);
  }
  if !tail.is_empty() {
    let k = tail
      .iter()
      .enumerate()
      .fold(0u32, |k, (i, b)| k | u32::from(*b) << (8 * i));
    h ^= k.wrapping_mul(C1).rotate_left(15).wrapping_mul(C2);
  }
  h ^= data.len() as u32;
  h ^= h >> 16;
  h = h.wrapping_mul(0x85eb_ca6b);
  h ^= h >> 13;
  h = h.wrapping_mul(0xc2b2_ae35);
  h ^ (h >> 16)
}

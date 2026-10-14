//! Fingerprints, the local fallback token, and the key recipes
//! `cactus.jd.com/request_algo` hands out with its tokens.

use std::sync::LazyLock;

use atp_core::{Error, ErrorCode, Result};
use rand::Rng;
use regex::Regex;

use super::crypto::{self, Hash};

/// Characters the library draws nonces from.
const NONCE_CHARS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHITUVWXYZ_-";
/// Characters a fingerprint is built from.
const FP_SOURCE: &str = "ekl9i1uct6";
const BASE36: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
/// Prefix of the local token's cipher.
const CIPHER_PREFIX: &str = "62";
/// Salt of the local key.
const LOCAL_KEY_SALT: &str = ")k$L61";

static PARAMS: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"function\s*\w*\s*\(([^)]*)\)").expect("valid"));
static STRINGS: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r#"var\s+(\w+)\s*=\s*['"]([^'"]*)['"]"#).expect("valid"));
static EXPRESSION: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"^[123]([x+][123])+").expect("valid"));
static CONCAT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\.concat\((\w+)\)").expect("valid"));
static CALL: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"return\s+\w+\.(MD5|SHA256|HmacSHA256|HmacMD5)\((\w+)(?:\s*,\s*(\w+))?\)")
    .expect("valid")
});

/// A random string of `len` nonce characters; the third is always `4`.
pub fn nonce(len: usize) -> String {
  let mut rng = rand::rng();
  let text: String = (0..len)
    .map(|_| NONCE_CHARS[rng.random_range(0..NONCE_CHARS.len())] as char)
    .collect();
  if text.len() > 2 {
    format!("{}4{}", &text[..2], &text[2..text.len() - 1])
  } else {
    text
  }
}

/// A new 16-character fingerprint: four source characters hidden between
/// fillers, the filler length last, the rest shifted and reversed.
pub fn fingerprint() -> String {
  let mut rng = rand::rng();
  let chosen = pick(FP_SOURCE, 4);
  let pool: Vec<char> = FP_SOURCE
    .chars()
    .filter(|c| !chosen.contains(*c))
    .map(|c| shift(c, 32))
    .collect();
  let size = rng.random_range(0..10usize);
  let mut filler = |n: usize| -> String {
    (0..n)
      .map(|_| pool[rng.random_range(0..pool.len())])
      .collect()
  };
  let raw: Vec<char> = format!("{}{chosen}{}{size}", filler(size), filler(11 - size))
    .chars()
    .collect();
  let head: String = raw[..15].iter().rev().map(|&c| shift(c, 13)).collect();
  head + &raw[15..].iter().collect::<String>()
}

/// `count` characters of `source` in source order, then shuffled.
fn pick(source: &str, count: usize) -> String {
  let mut rng = rand::rng();
  let chars: Vec<char> = source.chars().collect();
  let mut picked = Vec::new();
  let mut wanted = count;
  for (i, &c) in chars.iter().enumerate() {
    if rng.random::<f64>() * ((chars.len() - i) as f64) < wanted as f64 {
      picked.push(c);
      wanted -= 1;
      if wanted == 0 {
        break;
      }
    }
  }
  let mut out = String::new();
  for n in 0..picked.len() {
    let k = rng.random_range(0..picked.len() - n);
    out.push(picked[k]);
    picked[k] = picked[picked.len() - n - 1];
  }
  out
}

/// A base-36 digit `d` as `(5d + add) mod 36`.
fn shift(c: char, add: u32) -> char {
  let digit = c.to_digit(36).unwrap_or(0);
  BASE36[((5 * digit + add) % 36) as usize] as char
}

/// The token the library makes up when the server gives none (`tk06w…`).
/// It travels in `request_algo` too; business APIs refuse to sign with it.
pub fn local_token(fp: &str, now_ms: i64) -> String {
  let expression = expression();
  let cipher = cipher(fp, now_ms);
  let check = Hash::Md5.text(&format!("tk06w41wl{expression}{cipher}"));
  format!("tk06w{}41l{expression}{cipher}", &check[..8])
}

/// Two to five digits joined by `+` / `x`, filled up to nine characters,
/// in plain base64: how the local key is to be derived.
fn expression() -> String {
  let mut rng = rand::rng();
  let digits = rng.random_range(2..6);
  let mut text = String::new();
  for i in 0..digits {
    text.push(['1', '2', '3'][rng.random_range(0..3)]);
    if i < digits - 1 {
      text.push(['+', 'x'][rng.random_range(0..2)]);
    }
  }
  if text.len() < 9 {
    let fill = nonce(32);
    text.push_str(&fill[..9 - text.len()]);
  }
  crypto::stringify(text.as_bytes())
}

fn cipher(fp: &str, now_ms: i64) -> String {
  let random = nonce(12);
  let time = (now_ms as u64).to_le_bytes();
  // The checksum sees the random part rotated by one.
  let mut rotated = random.as_bytes().to_vec();
  rotated.rotate_right(1);
  let mut input = CIPHER_PREFIX.as_bytes().to_vec();
  input.extend(&rotated);
  input.extend(time);
  input.extend(fp.as_bytes());
  let check = crypto::hex(&Hash::Md5.digest(&input));
  let mut raw = check.as_bytes()[..8].to_vec();
  raw.extend(CIPHER_PREFIX.as_bytes());
  raw.extend(random.as_bytes());
  raw.extend(time);
  raw.extend(fp.as_bytes());
  crypto::encode(&raw)
}

/// The signing key of a local token: its expression (`1+2x3`) names the
/// hashes (1 MD5, 2 SHA-256, 3 HMAC) to concatenate (`+`) or chain (`x`).
pub fn local_key(token: &str, fp: &str, time: &str, app_id: &str) -> Result<String> {
  let input = format!("{token}{fp}{time}{app_id}{LOCAL_KEY_SALT}");
  let expression = token
    .get(16..28)
    .and_then(unstringify)
    .ok_or_else(|| Error::internal("malformed local h5st token"))?;
  let mut key = String::new();
  let mut chain = false;
  for c in expression.chars() {
    match c {
      '+' => chain = false,
      'x' => chain = true,
      '1'..='3' => {
        let data = if chain { key.clone() } else { input.clone() };
        let part = match c {
          '1' => Hash::Md5.text(&data),
          '2' => Hash::Sha256.text(&data),
          _ => Hash::Sha256.hmac(&data, token),
        };
        key = if chain { part } else { key + &part };
      }
      _ => break,
    }
  }
  Ok(key)
}

/// The `[123]([x+][123])+` a token's middle decodes to.
fn unstringify(text: &str) -> Option<String> {
  let mut bits = 0u32;
  let mut count = 0;
  let mut out = Vec::new();
  for c in text.bytes() {
    bits = bits << 6 | crypto::ALPHABET.iter().position(|&a| a == c)? as u32;
    count += 6;
    if count >= 8 {
      count -= 8;
      out.push((bits >> count & 0xff) as u8);
    }
  }
  let text = String::from_utf8_lossy(&out);
  EXPRESSION.find(&text).map(|m| m.as_str().to_owned())
}

/// How the server's `algo` function derives the key: its inputs in order
/// (parameters or string constants), the hash, and the HMAC key argument.
#[derive(Debug, Clone)]
pub struct Recipe {
  inputs: Vec<Input>,
  hash: Hash,
  hmac_key: Option<Input>,
}

#[derive(Debug, Clone)]
enum Input {
  /// Index into (token, fingerprint, time, app id).
  Param(usize),
  Text(String),
}

/// Values a recipe is applied to.
pub struct KeyInputs<'a> {
  pub token: &'a str,
  pub fp: &'a str,
  pub time: &'a str,
  pub app_id: &'a str,
}

impl Recipe {
  /// Read an `algo` function such as `function test(tk,fp,ts,ai,algo){var
  /// rd='…';var str="".concat(tk).concat(fp)…;return algo.SHA256(str);}`
  /// without running it; other shapes are refused.
  pub fn parse(algo: &str) -> Result<Recipe> {
    let unknown = || {
      Error::new(
        ErrorCode::SignatureError,
        "JD's request_algo returned a key function atp cannot read",
      )
      .with_hint(format!("algo: {}", atp_core::text::truncate(algo, 200)))
    };
    let params: Vec<String> = PARAMS.captures(algo).ok_or_else(unknown)?[1]
      .split(',')
      .map(|p| p.trim().to_owned())
      .collect();
    let strings: Vec<(String, String)> = STRINGS
      .captures_iter(algo)
      .map(|m| (m[1].to_owned(), m[2].to_owned()))
      .collect();
    let resolve = |name: &str| -> Option<Input> {
      if let Some(i) = params.iter().position(|p| p == name).filter(|&i| i < 4) {
        return Some(Input::Param(i));
      }
      strings
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| Input::Text(v.clone()))
    };
    let inputs: Option<Vec<Input>> = CONCAT.captures_iter(algo).map(|m| resolve(&m[1])).collect();
    let inputs = inputs.filter(|i| !i.is_empty()).ok_or_else(unknown)?;
    let call = CALL.captures(algo).ok_or_else(unknown)?;
    let (hash, hmac) = match &call[1] {
      "MD5" => (Hash::Md5, false),
      "SHA256" => (Hash::Sha256, false),
      "HmacMD5" => (Hash::Md5, true),
      _ => (Hash::Sha256, true),
    };
    let hmac_key = match call.get(3) {
      Some(arg) if hmac => Some(resolve(arg.as_str()).ok_or_else(unknown)?),
      _ if hmac => return Err(unknown()),
      _ => None,
    };
    Ok(Recipe {
      inputs,
      hash,
      hmac_key,
    })
  }

  pub fn key(&self, values: &KeyInputs) -> String {
    let value = |input: &Input| match input {
      Input::Param(0) => values.token.to_owned(),
      Input::Param(1) => values.fp.to_owned(),
      Input::Param(2) => values.time.to_owned(),
      Input::Param(_) => values.app_id.to_owned(),
      Input::Text(text) => text.clone(),
    };
    let data: String = self.inputs.iter().map(value).collect();
    match &self.hmac_key {
      Some(key) => self.hash.hmac(&data, &value(key)),
      None => self.hash.text(&data),
    }
  }
}

//! Credentials that are not cookies: bot tokens, API keys and app secrets,
//! from environment variables or from `login --token`, kept in the session
//! extras (the session file is private to the user).

use crate::ctx::Ctx;
use crate::error::{Error, Result};

/// A non-empty environment variable, trimmed.
pub fn env(name: &str) -> Option<String> {
  std::env::var(name)
    .ok()
    .map(|v| v.trim().to_owned())
    .filter(|v| !v.is_empty())
}

/// The first of several environment variables that is set.
pub fn env_any(names: &[&str]) -> Option<String> {
  names.iter().find_map(|n| env(n))
}

/// The fields of a `login --token` value. They are separated by `|` when
/// there is one, else by whitespace, else by `:`; a lone URL stays whole.
pub fn fields(token: &str) -> Vec<String> {
  let token = token.trim();
  let parts: Vec<&str> = if token.contains('|') {
    token.split('|').collect()
  } else if token.contains(char::is_whitespace) {
    token.split_whitespace().collect()
  } else if token.contains("://") {
    vec![token]
  } else {
    token.split(':').collect()
  };
  parts.iter().map(|p| p.trim().to_owned()).collect()
}

/// `fields`, checked to number between `min` and `max`; `form` names the
/// expected shape for the error (`APP_ID:APP_SECRET`).
pub fn expect(token: &str, min: usize, max: usize, form: &str) -> Result<Vec<String>> {
  let fields = fields(token);
  if fields.len() < min || fields.len() > max || fields.iter().any(String::is_empty) {
    return Err(Error::input(format!("expected `login --token '{form}'`")));
  }
  Ok(fields)
}

/// Session extras saved together: all of them, or `None` when one is missing.
pub fn saved(ctx: &Ctx, keys: &[&str]) -> Option<Vec<String>> {
  keys
    .iter()
    .map(|k| ctx.extra(k).filter(|v| !v.is_empty()))
    .collect()
}

/// Keep `(key, value)` pairs in the session extras.
pub fn save(ctx: &Ctx, pairs: &[(&str, &str)]) {
  for (k, v) in pairs {
    ctx.set_extra(k, v);
  }
}

/// A key or token for display: its first and last four characters.
pub fn mask(secret: &str) -> String {
  let chars: Vec<char> = secret.chars().collect();
  if chars.len() <= 8 {
    return "*".repeat(chars.len());
  }
  let head: String = chars[..4].iter().collect();
  let tail: String = chars[chars.len() - 4..].iter().collect();
  format!("{head}…{tail}")
}

/// The host of a server given as a host or a URL (instances of federated
/// platforms): `https://misskey.io/` or `misskey.io` → `misskey.io`.
pub fn host(url: &str) -> String {
  let url = url.trim();
  let url = url
    .strip_prefix("https://")
    .or_else(|| url.strip_prefix("http://"))
    .unwrap_or(url);
  url.split('/').next().unwrap_or(url).to_ascii_lowercase()
}

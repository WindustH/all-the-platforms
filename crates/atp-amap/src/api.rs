//! Amap Web Service transport (`https://restapi.amap.com`). Every request
//! carries the key and, when a signing secret is set, `sig`. Credentials
//! come from `AMAP_KEY` / `AMAP_SIGN_SECRET` (they win) or `login --token
//! KEY[:SIGN_SECRET]`; `infocode`s map onto atp errors.

use atp_core::credentials;
use atp_core::http::{Req, Resp};
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt};

use crate::sign;

pub const REST: &str = "https://restapi.amap.com";
const KEY: &str = "key";
const SECRET: &str = "sign_secret";
const LOGIN_HINT: &str = "run `atp amap login --token KEY[:SIGN_SECRET]` with a key of the \
   「Web服务」 platform from https://console.amap.com/dev/key/app";

/// A Web Service key and its optional signing secret.
#[derive(Clone)]
pub struct Creds {
  pub key: String,
  pub secret: Option<String>,
}

pub struct Api {
  pub ctx: Ctx,
  env: Option<Creds>,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    let env = credentials::env("AMAP_KEY").map(|key| Creds {
      key,
      secret: credentials::env("AMAP_SIGN_SECRET"),
    });
    Self { ctx, env }
  }

  pub fn creds(&self) -> Option<Creds> {
    if let Some(c) = &self.env {
      return Some(c.clone());
    }
    let key = self.ctx.extra(KEY).filter(|k| !k.is_empty())?;
    let secret = self.ctx.extra(SECRET).filter(|s| !s.is_empty());
    Some(Creds { key, secret })
  }

  pub fn logged_in(&self) -> bool {
    self.creds().is_some()
  }

  pub fn require(&self) -> Result<Creds> {
    self
      .creds()
      .ok_or_else(|| Error::auth("no Amap Web Service key").with_hint(LOGIN_HINT))
  }

  /// Keep `KEY[:SIGN_SECRET]` in the session.
  pub fn save(&self, token: &str) -> Result<()> {
    let fields = credentials::expect(token, 1, 2, "KEY[:SIGN_SECRET]")?;
    let secret = fields.get(1).map(String::as_str).unwrap_or_default();
    credentials::save(&self.ctx, &[(KEY, &fields[0]), (SECRET, secret)]);
    Ok(())
  }

  /// GET a JSON endpoint; empty parameters are left out.
  pub async fn get(&self, path: &str, params: &[(&str, String)]) -> Result<Value> {
    check(self.request(path, params)?.send().await?)
  }

  /// GET an endpoint that answers with a file (static maps); errors still
  /// come back as JSON.
  pub async fn bytes(&self, path: &str, params: &[(&str, String)]) -> Result<Vec<u8>> {
    let resp = self.request(path, params)?.send().await?;
    let json = resp
      .header("content-type")
      .is_some_and(|t| t.contains("json"));
    if json {
      check(resp)?;
      return Err(Error::upstream(
        "Amap answered with JSON instead of an image",
      ));
    }
    Ok(resp.check()?.body)
  }

  fn request(&self, path: &str, params: &[(&str, String)]) -> Result<Req<'_>> {
    let creds = self.require()?;
    let mut pairs: Vec<(String, String)> = params
      .iter()
      .filter(|(_, v)| !v.is_empty())
      .map(|(k, v)| ((*k).to_owned(), v.clone()))
      .collect();
    pairs.push(("key".into(), creds.key));
    if let Some(secret) = &creds.secret {
      let sig = sign::sig(&pairs, secret);
      pairs.push(("sig".into(), sig));
    }
    let url = format!("{REST}{path}");
    Ok(self.ctx.http.get(url).no_cookies().queries(pairs))
  }
}

fn check(resp: Resp) -> Result<Value> {
  let v = resp.check()?.value()?;
  if v.str("status").as_deref() == Some("1") {
    return Ok(v);
  }
  Err(error(&v))
}

/// The atp error for a failed answer, by its `infocode`.
fn error(v: &Value) -> Error {
  let info = v.str("info").unwrap_or_default();
  let code = v.str("infocode").unwrap_or_default();
  let message = format!("Amap: {info} ({code})");
  match code.as_str() {
    "10001" | "10008" | "10009" | "10013" => Error::auth(message).with_hint(LOGIN_HINT),
    "10003" | "10004" | "10010" | "10014" | "10019" | "10020" | "10021" | "10029" | "10044" => {
      Error::new(ErrorCode::RateLimited, message)
        .with_hint("wait a moment, or raise the key's quota in the Amap console")
    }
    "10007" => Error::new(ErrorCode::SignatureError, message).with_hint(
      "check AMAP_SIGN_SECRET or the secret after `:` in `login --token` (the key's 数字签名 private key)",
    ),
    "10005" | "10006" => Error::new(ErrorCode::PermissionDenied, message)
      .with_hint("this machine's IP is not in the key's whitelist (console → key settings)"),
    "10012" | "10041" | "20011" => Error::new(ErrorCode::PermissionDenied, message)
      .with_hint("the key has no permission for this service; use a 「Web服务」 key"),
    "10015" | "10016" | "10017" => Error::network(message),
    "10026" | "20000" | "20001" | "20002" | "20012" => Error::input(message),
    "20800" | "20801" | "20802" | "20803" => Error::not_found(message),
    _ => Error::upstream(message),
  }
}

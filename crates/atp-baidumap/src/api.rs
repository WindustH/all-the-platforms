//! Baidu Map Web Service transport (`https://api.map.baidu.com`). Every
//! request carries `ak` and asks for JSON; with an SK (SN verification)
//! it adds `timestamp` and signs the query string with `sn`, last.
//! Credentials come from `BAIDU_MAP_AK` / `BAIDU_MAP_SK` (they win) or
//! `login --token AK[:SK]`; `status` codes map onto atp errors.

use atp_core::credentials;
use atp_core::geo::System;
use atp_core::http::Resp;
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt};

use crate::sign;

pub const API: &str = "https://api.map.baidu.com";
const AK: &str = "ak";
const SK: &str = "sk";
const KEYS: &str = "with a 服务端 AK from https://lbsyun.baidu.com/apiconsole/key";

/// An AK and, for SN verification, its SK.
#[derive(Clone)]
pub struct Creds {
  pub ak: String,
  pub sk: Option<String>,
}

pub struct Api {
  pub ctx: Ctx,
  env: Option<Creds>,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    let env = credentials::env("BAIDU_MAP_AK").map(|ak| Creds {
      ak,
      sk: credentials::env("BAIDU_MAP_SK"),
    });
    Self { ctx, env }
  }

  pub fn creds(&self) -> Option<Creds> {
    if let Some(c) = &self.env {
      return Some(c.clone());
    }
    let ak = self.ctx.extra(AK).filter(|k| !k.is_empty())?;
    let sk = self.ctx.extra(SK).filter(|s| !s.is_empty());
    Some(Creds { ak, sk })
  }

  pub fn logged_in(&self) -> bool {
    self.creds().is_some()
  }

  pub fn require(&self) -> Result<Creds> {
    self
      .creds()
      .ok_or_else(|| Error::auth("no Baidu Map AK").with_hint(self.login_hint()))
  }

  /// How to log in, and where keys come from.
  fn login_hint(&self) -> String {
    format!("{} {KEYS}", self.ctx.login_hint())
  }

  /// Keep `AK[:SK]` in the session.
  pub fn save(&self, token: &str) -> Result<()> {
    let fields = credentials::expect(token, 1, 2, "AK[:SK]")?;
    let sk = fields.get(1).map(String::as_str).unwrap_or_default();
    credentials::save(&self.ctx, &[(AK, &fields[0]), (SK, sk)]);
    Ok(())
  }

  /// The system of the points users give and see (`--coord`).
  pub fn system(&self) -> System {
    self.ctx.coord.unwrap_or(System::Bd09)
  }

  /// The system points travel to and from Baidu in: BD-09 when users work
  /// in it, else GCJ-02 (`coord_type` / `ret_coordtype`), which converts
  /// to WGS-84 exactly.
  pub fn wire(&self) -> System {
    match self.system() {
      System::Bd09 => System::Bd09,
      _ => System::Gcj02,
    }
  }

  /// GET a JSON endpoint; empty parameters are left out.
  pub async fn get(&self, path: &str, params: &[(&str, String)]) -> Result<Value> {
    let resp = self.request(path, params, true)?.send().await?;
    check(resp.check()?.value()?, &self.login_hint())
  }

  /// GET an endpoint that answers with an image (static maps); errors
  /// come back as JSON.
  pub async fn image(&self, path: &str, params: &[(&str, String)]) -> Result<Vec<u8>> {
    let resp = self.request(path, params, false)?.send().await?;
    let image = resp
      .header("content-type")
      .is_some_and(|t| t.starts_with("image/"));
    if image {
      return Ok(resp.check()?.body);
    }
    let v = error_body(&resp)?;
    check(v, &self.login_hint())?;
    Err(Error::upstream("Baidu answered without an image"))
  }

  fn request(
    &self,
    path: &str,
    params: &[(&str, String)],
    json: bool,
  ) -> Result<atp_core::http::Req<'_>> {
    let creds = self.require()?;
    let mut pairs: Vec<(String, String)> = params
      .iter()
      .filter(|(_, v)| !v.is_empty())
      .map(|(k, v)| ((*k).to_owned(), v.clone()))
      .collect();
    if json {
      pairs.push(("output".into(), "json".into()));
    }
    pairs.push(("ak".into(), creds.ak));
    let mut query = if let Some(sk) = &creds.sk {
      let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
      pairs.push(("timestamp".into(), now.to_string()));
      let query = sign::query(&pairs);
      let sn = sign::sn(path, &query, sk);
      format!("{query}&sn={sn}")
    } else {
      sign::query(&pairs)
    };
    query.insert(0, '?');
    let url = format!("{API}{path}{query}");
    Ok(self.ctx.http.get(url).no_cookies())
  }
}

fn error_body(resp: &Resp) -> Result<Value> {
  resp.value().map_err(|_| {
    Error::upstream(format!(
      "Baidu: {}",
      atp_core::text::truncate(&resp.text(), 200)
    ))
  })
}

fn check(v: Value, login: &str) -> Result<Value> {
  match v.i64("status") {
    Some(0) => Ok(v),
    Some(code) => Err(error(code, &v, login)),
    None => Err(Error::upstream("Baidu answered without a status")),
  }
}

/// The atp error for a failed answer, by its `status`.
fn error(code: i64, v: &Value, login: &str) -> Error {
  let text = v.first_str(&["message", "msg"]).unwrap_or_default();
  let message = format!("Baidu Map: {text} ({code})");
  match code {
    3 | 5 | 101 | 200..=202 | 250..=252 | 500 => Error::auth(message).with_hint(login),
    4 | 301 | 302 | 401 | 402 => Error::new(ErrorCode::RateLimited, message)
      .with_hint("wait a moment, or raise the AK's quota in the Baidu console"),
    211 => Error::new(ErrorCode::SignatureError, message).with_hint(
      "check BAIDU_MAP_SK or the SK after `:` in `login --token` (the AK's SN check key)",
    ),
    102 | 210 | 220 | 230 => Error::new(ErrorCode::PermissionDenied, message)
      .with_hint("this machine's IP is not in the AK's whitelist (console → AK settings)"),
    203 => Error::new(ErrorCode::PermissionDenied, message)
      .with_hint("the AK is not a 服务端 AK; create one for the server platform"),
    9 | 240 | 260 | 261 => Error::new(ErrorCode::PermissionDenied, message)
      .with_hint("the AK lacks this service; enable it in the Baidu console"),
    2 | 8 | 502..=506 => Error::input(message),
    7 | 1001..=1003 | 2001 => Error::not_found(message),
    1 => Error::network(message),
    _ => Error::upstream(message),
  }
}

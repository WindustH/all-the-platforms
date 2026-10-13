//! Satori transport: `POST <base>/v1/<resource>.<method>` with JSON, the
//! account to act as in `Satori-Platform` / `Satori-User-ID` (and the older
//! `X-Platform` / `X-Self-ID`), and `Authorization: Bearer` when the server
//! has a token. The logins a server offers arrive in the READY signal of
//! its event websocket (`<base>/v1/events`).
//!
//! The server comes from `SATORI_URL` / `SATORI_TOKEN` / `SATORI_PLATFORM` /
//! `SATORI_USER_ID` when set, else from the session ([`crate::login`]).

use std::time::Duration;

use atp_core::credentials;
use atp_core::http::Resp;
use atp_core::socket::Socket;
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt, json};

const KEYS: [&str; 4] = ["url", "token", "platform", "user_id"];

#[derive(Debug, Clone, Default)]
pub struct Server {
  /// Base URL without `/v1`, e.g. `http://127.0.0.1:5140/satori`.
  pub base: String,
  pub token: Option<String>,
  pub platform: Option<String>,
  pub user_id: Option<String>,
}

impl Server {
  /// `URL[|TOKEN][|PLATFORM:USER_ID]`.
  pub fn parse(raw: &str) -> Result<Self> {
    let fields = credentials::fields(raw);
    let url = fields.first().map(String::as_str).unwrap_or_default();
    if !url.starts_with("http://") && !url.starts_with("https://") {
      return Err(Error::input(format!(
        "not a Satori address: {url:?}; expected `http://host:port/satori[|TOKEN][|PLATFORM:USER_ID]`"
      )));
    }
    let token = fields.get(1).filter(|t| !t.is_empty()).cloned();
    let (platform, user_id) = match fields.get(2).and_then(|a| a.split_once(':')) {
      Some((p, u)) => (Some(p.to_owned()), Some(u.to_owned())),
      None => (None, None),
    };
    Ok(Self {
      base: base(url),
      token,
      platform,
      user_id,
    })
  }

  fn events_url(&self) -> String {
    let ws = self
      .base
      .replacen("https://", "wss://", 1)
      .replacen("http://", "ws://", 1);
    format!("{ws}/v1/events")
  }
}

/// `…/satori`, `…/satori/v1` and `…/satori/v1/events` all name one server.
fn base(url: &str) -> String {
  let mut base = url.trim_end_matches('/');
  for suffix in ["/events", "/v1"] {
    base = base.strip_suffix(suffix).unwrap_or(base);
  }
  base.to_owned()
}

pub struct Api {
  pub ctx: Ctx,
  env: Option<Server>,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    let env = credentials::env("SATORI_URL").map(|url| Server {
      base: base(&url),
      token: credentials::env("SATORI_TOKEN"),
      platform: credentials::env("SATORI_PLATFORM"),
      user_id: credentials::env("SATORI_USER_ID"),
    });
    Self { ctx, env }
  }

  pub fn server(&self) -> Option<Server> {
    if let Some(s) = &self.env {
      return Some(s.clone());
    }
    let get = |k: &str| self.ctx.extra(k).filter(|v| !v.is_empty());
    Some(Server {
      base: get(KEYS[0])?,
      token: get(KEYS[1]),
      platform: get(KEYS[2]),
      user_id: get(KEYS[3]),
    })
  }

  pub fn logged_in(&self) -> bool {
    self.server().is_some()
  }

  pub fn require(&self) -> Result<Server> {
    self.server().ok_or_else(|| {
      Error::auth("no Satori server configured").with_hint(
        "run `atp satori login --token 'http://127.0.0.1:5140/satori|TOKEN'` or set SATORI_URL",
      )
    })
  }

  pub fn save(&self, server: &Server) {
    let values = [
      Some(server.base.as_str()),
      server.token.as_deref(),
      server.platform.as_deref(),
      server.user_id.as_deref(),
    ];
    let pairs: Vec<(&str, &str)> = KEYS
      .iter()
      .zip(values)
      .map(|(k, v)| (*k, v.unwrap_or_default()))
      .collect();
    credentials::save(&self.ctx, &pairs);
  }

  pub async fn call(&self, method: &str, body: Value) -> Result<Value> {
    let server = self.require()?;
    self.call_at(&server, method, body).await
  }

  pub async fn call_at(&self, server: &Server, method: &str, body: Value) -> Result<Value> {
    let mut req = self
      .ctx
      .http
      .post(format!("{}/v1/{method}", server.base))
      .no_cookies()
      .no_throttle()
      .json(&body);
    if let Some(t) = &server.token {
      req = req.header("authorization", format!("Bearer {t}"));
    }
    if let (Some(p), Some(u)) = (&server.platform, &server.user_id) {
      req = req
        .header("satori-platform", p)
        .header("satori-user-id", u)
        .header("x-platform", p)
        .header("x-self-id", u);
    }
    check(req.send().await?, method)
  }

  /// The event websocket, identified; returns it with the READY body.
  pub async fn events(&self, server: &Server, sn: Option<u64>) -> Result<(Socket, Value)> {
    let mut socket = self
      .ctx
      .http
      .websocket(&server.events_url(), &[], false)
      .await?;
    let mut body = json!({"token": server.token.clone().unwrap_or_default()});
    if let Some(sn) = sn {
      body["sn"] = json!(sn);
    }
    socket.send_json(&json!({"op": 3, "body": body})).await?;
    let ready = async {
      while let Some(v) = socket.next_json().await? {
        if v.u64("op") == Some(4) {
          return Ok(v.at("body").clone());
        }
      }
      Err(Error::auth(
        "the Satori server closed the event connection (wrong token?)",
      ))
    };
    let body = tokio::time::timeout(Duration::from_secs(15), ready)
      .await
      .map_err(|_| Error::network("no READY from the Satori server within 15 s"))??;
    Ok((socket, body))
  }

  /// The accounts the server offers (from READY).
  pub async fn logins(&self, server: &Server) -> Result<Vec<Value>> {
    let (_, ready) = self.events(server, None).await?;
    Ok(ready.list("logins").to_vec())
  }
}

/// The JSON of a success (`Null` when empty), or the mapped error.
fn check(resp: Resp, method: &str) -> Result<Value> {
  let status = resp.status.as_u16();
  if resp.status.is_success() {
    if resp.body.iter().all(u8::is_ascii_whitespace) {
      return Ok(Value::Null);
    }
    return resp.value();
  }
  let detail = resp.text();
  let detail = detail.trim();
  Err(match status {
    401 => Error::auth("the Satori server refused the token")
      .with_hint("check the token in the server's Satori settings"),
    403 => Error::new(
      ErrorCode::PermissionDenied,
      format!("`{method}` is not allowed: {detail}"),
    ),
    404 | 405 | 501 => Error::new(
      ErrorCode::UnsupportedOperation,
      format!("the Satori server or its platform has no `{method}`"),
    ),
    _ => Error::upstream(format!("`{method}` failed (HTTP {status}): {detail}")),
  })
}

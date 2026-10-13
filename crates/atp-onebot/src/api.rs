//! OneBot v11 transport: actions of an implementation the user runs
//! (NapCat, Lagrange.OneBot, LLOneBot, go-cqhttp ...).
//!
//! The endpoint comes from `ONEBOT_URL` / `ONEBOT_TOKEN` / `ONEBOT_WS_URL`
//! when set, else from `login --token 'URL[|ACCESS_TOKEN][|WS_URL]'`. An
//! `http(s)://` URL takes actions as `POST <url>/<action>`; a `ws(s)://` URL
//! (the forward websocket) takes them as `{action, params, echo}` frames and
//! also carries the events `listen` prints. Replies are
//! `{status, retcode, data, message, wording}`.

use std::time::Duration;

use atp_core::socket::Socket;
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt, json};

const URL_KEY: &str = "url";
const TOKEN_KEY: &str = "token";
const WS_KEY: &str = "ws";
const ACTION_TIMEOUT: Duration = Duration::from_secs(30);

/// Where the implementation listens.
#[derive(Debug, Clone, Default)]
pub struct Endpoint {
  /// `http(s)://…` for actions, or `ws(s)://…` for actions and events.
  pub url: String,
  pub token: Option<String>,
  /// Forward websocket for events, when `url` is HTTP.
  pub ws: Option<String>,
}

impl Endpoint {
  /// `URL[|ACCESS_TOKEN][|WS_URL]`.
  pub fn parse(raw: &str) -> Result<Self> {
    let mut parts = raw.split('|').map(str::trim);
    let url = parts
      .next()
      .unwrap_or_default()
      .trim_end_matches('/')
      .to_owned();
    if !["http://", "https://", "ws://", "wss://"]
      .iter()
      .any(|s| url.starts_with(s))
    {
      return Err(Error::input(format!(
        "not a OneBot address: {url:?}; expected `http://host:port[|ACCESS_TOKEN][|ws://host:port]`"
      )));
    }
    let token = parts.next().filter(|t| !t.is_empty()).map(str::to_owned);
    let ws = parts
      .next()
      .filter(|w| w.starts_with("ws://") || w.starts_with("wss://"))
      .map(|w| w.trim_end_matches('/').to_owned());
    Ok(Self { url, token, ws })
  }

  fn is_ws(&self) -> bool {
    self.url.starts_with("ws")
  }

  /// The forward websocket, if one is known.
  pub fn events(&self) -> Option<&str> {
    if self.is_ws() {
      Some(&self.url)
    } else {
      self.ws.as_deref()
    }
  }
}

pub struct Api {
  pub ctx: Ctx,
  env: Option<Endpoint>,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    let var = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
    let env = var("ONEBOT_URL").and_then(|url| {
      let mut e = Endpoint::parse(&url).ok()?;
      e.token = var("ONEBOT_TOKEN").or(e.token);
      e.ws = var("ONEBOT_WS_URL").or(e.ws);
      Some(e)
    });
    Self { ctx, env }
  }

  pub fn endpoint(&self) -> Option<Endpoint> {
    if let Some(e) = &self.env {
      return Some(e.clone());
    }
    let url = self.ctx.extra(URL_KEY).filter(|u| !u.is_empty())?;
    Some(Endpoint {
      url,
      token: self.ctx.extra(TOKEN_KEY).filter(|t| !t.is_empty()),
      ws: self.ctx.extra(WS_KEY).filter(|w| !w.is_empty()),
    })
  }

  pub fn logged_in(&self) -> bool {
    self.endpoint().is_some()
  }

  pub fn require(&self) -> Result<Endpoint> {
    self.endpoint().ok_or_else(|| {
      Error::auth("no OneBot implementation configured").with_hint(
        "run `atp onebot login --token 'http://127.0.0.1:3000|ACCESS_TOKEN'` (the HTTP or \
         websocket address of NapCat / Lagrange / LLOneBot) or set ONEBOT_URL",
      )
    })
  }

  pub fn save(&self, e: &Endpoint) {
    self.ctx.set_extra(URL_KEY, &e.url);
    self
      .ctx
      .set_extra(TOKEN_KEY, e.token.as_deref().unwrap_or_default());
    self
      .ctx
      .set_extra(WS_KEY, e.ws.as_deref().unwrap_or_default());
  }

  /// One action; its `data` on success.
  pub async fn call(&self, action: &str, params: Value) -> Result<Value> {
    let e = self.require()?;
    self.call_at(&e, action, params).await
  }

  pub async fn call_at(&self, e: &Endpoint, action: &str, params: Value) -> Result<Value> {
    let reply = if e.is_ws() {
      self.call_ws(e, action, params).await?
    } else {
      self.call_http(e, action, params).await?
    };
    check(action, &reply)
  }

  async fn call_http(&self, e: &Endpoint, action: &str, params: Value) -> Result<Value> {
    let mut req = self
      .ctx
      .http
      .post(format!("{}/{action}", e.url))
      .no_cookies()
      .no_throttle()
      .json(&params);
    if let Some(t) = &e.token {
      req = req.header("authorization", format!("Bearer {t}"));
    }
    let resp = req.send().await?;
    match resp.status.as_u16() {
      401 | 403 => {
        return Err(
          Error::auth("the OneBot implementation refused the access token")
            .with_hint("check the token in the implementation's HTTP settings"),
        );
      }
      404 => return Err(unsupported(action)),
      _ => {}
    }
    resp.value().map_err(|_| {
      Error::upstream(format!(
        "{} did not answer like a OneBot implementation (HTTP {})",
        e.url, resp.status
      ))
    })
  }

  async fn call_ws(&self, e: &Endpoint, action: &str, params: Value) -> Result<Value> {
    let mut ws = self.connect(&e.url, e.token.as_deref()).await?;
    let echo = format!("atp-{}", rand::random::<u32>());
    ws.send_json(&json!({"action": action, "params": params, "echo": echo}))
      .await?;
    let wait = async {
      while let Some(frame) = ws.next_json().await? {
        if frame.str("echo").as_deref() == Some(echo.as_str()) {
          return Ok(frame);
        }
      }
      Err(Error::network(
        "the OneBot websocket closed before answering",
      ))
    };
    tokio::time::timeout(ACTION_TIMEOUT, wait)
      .await
      .map_err(|_| Error::network(format!("no answer to `{action}` within 30 s")))?
  }

  pub async fn connect(&self, url: &str, token: Option<&str>) -> Result<Socket> {
    let auth = token.map(|t| format!("Bearer {t}"));
    let headers: Vec<(&str, &str)> = auth
      .as_deref()
      .map(|a| vec![("Authorization", a)])
      .unwrap_or_default();
    self
      .ctx
      .http
      .websocket(url, &headers, false)
      .await
      .map_err(|e| {
        if e.message.contains("401") || e.message.contains("403") {
          Error::auth("the OneBot websocket refused the access token")
        } else {
          e
        }
      })
  }
}

fn unsupported(action: &str) -> Error {
  Error::new(
    ErrorCode::UnsupportedOperation,
    format!("this OneBot implementation has no `{action}` action"),
  )
  .with_hint("NapCat and Lagrange.OneBot implement the most extensions")
}

/// `{status: "ok", data}` → data; failures → an error naming the action.
fn check(action: &str, reply: &Value) -> Result<Value> {
  if reply.str("status").as_deref() == Some("ok") || reply.i64("retcode") == Some(0) {
    return Ok(reply.at("data").clone());
  }
  let code = reply.i64("retcode").unwrap_or(-1);
  if code == 1404 {
    return Err(unsupported(action));
  }
  let message = reply
    .first_str(&["wording", "message", "msg"])
    .unwrap_or_else(|| format!("retcode {code}"));
  Err(Error::upstream(format!("`{action}` failed: {message}")))
}

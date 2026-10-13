//! QQ bot transport (`api.sgroup.qq.com`, or the sandbox).
//!
//! The bot's AppID and AppSecret come from `QQ_BOT_APPID` / `QQ_BOT_SECRET`
//! (`QQ_BOT_SANDBOX=1` for the sandbox) when set, else from
//! `login --token APPID:SECRET[:sandbox]`. They buy an access token
//! (`bots.qq.com/app/getAppAccessToken`, about two hours), cached privately
//! until shortly before it expires and sent as `Authorization: QQBot …`.
//! Errors are `{code, message, err_code, trace_id}`.

use atp_core::credentials;
use atp_core::http::{Method, Part};
use atp_core::token::AccessToken;
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt, json};

const TOKEN_URL: &str = "https://bots.qq.com/app/getAppAccessToken";
const API: &str = "https://api.sgroup.qq.com";
const SANDBOX: &str = "https://sandbox.api.sgroup.qq.com";
const KEYS: [&str; 3] = ["appid", "secret", "sandbox"];

#[derive(Debug, Clone)]
pub struct Credentials {
  pub appid: String,
  secret: String,
  pub sandbox: bool,
}

impl Credentials {
  /// `APPID:SECRET[:sandbox]`.
  pub fn parse(raw: &str) -> Result<Self> {
    let fields = credentials::fields(raw);
    let mut parts = fields.iter().map(String::as_str);
    let (Some(appid), Some(secret)) = (parts.next(), parts.next()) else {
      return Err(Error::input(
        "expected APPID:SECRET (from q.qq.com → 开发 → 开发设置), optionally :sandbox",
      ));
    };
    if appid.is_empty() || secret.is_empty() || !appid.bytes().all(|b| b.is_ascii_digit()) {
      return Err(Error::input("expected APPID:SECRET with a numeric AppID"));
    }
    Ok(Self {
      appid: appid.to_owned(),
      secret: secret.to_owned(),
      sandbox: parts.next() == Some("sandbox"),
    })
  }
}

#[derive(Clone, Copy)]
pub enum Body<'a> {
  None,
  Json(&'a Value),
}

pub struct Api {
  pub ctx: Ctx,
  env: Option<Credentials>,
  token: AccessToken,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    let env = match (
      credentials::env("QQ_BOT_APPID"),
      credentials::env("QQ_BOT_SECRET"),
    ) {
      (Some(appid), Some(secret)) => Some(Credentials {
        appid,
        secret,
        sandbox: credentials::env("QQ_BOT_SANDBOX").is_some_and(|v| v != "0"),
      }),
      _ => None,
    };
    Self {
      ctx,
      env,
      token: AccessToken::default(),
    }
  }

  pub fn credentials(&self) -> Option<Credentials> {
    if let Some(c) = &self.env {
      return Some(c.clone());
    }
    let get = |k: &str| self.ctx.extra(k).filter(|v| !v.is_empty());
    Some(Credentials {
      appid: get(KEYS[0])?,
      secret: get(KEYS[1])?,
      sandbox: get(KEYS[2]).as_deref() == Some("1"),
    })
  }

  pub fn logged_in(&self) -> bool {
    self.credentials().is_some()
  }

  pub fn require(&self) -> Result<Credentials> {
    self.credentials().ok_or_else(|| {
      Error::auth("no QQ bot credentials").with_hint(
        "run `atp qq login --token 'APPID:SECRET'` (from q.qq.com) or set QQ_BOT_APPID and QQ_BOT_SECRET",
      )
    })
  }

  pub fn save(&self, c: &Credentials) {
    self.ctx.set_extra(KEYS[0], &c.appid);
    self.ctx.set_extra(KEYS[1], &c.secret);
    self
      .ctx
      .set_extra(KEYS[2], if c.sandbox { "1" } else { "0" });
  }

  fn base(c: &Credentials) -> &'static str {
    if c.sandbox { SANDBOX } else { API }
  }

  /// A valid access token: memory, cache, or a new one.
  pub async fn access_token(&self, c: &Credentials) -> Result<String> {
    let key = token_key(c);
    self
      .token
      .get(&self.ctx, &key, async || self.fetch_token(c).await)
      .await
  }

  async fn fetch_token(&self, c: &Credentials) -> Result<(String, i64)> {
    let v = self
      .ctx
      .http
      .post(TOKEN_URL)
      .no_cookies()
      .json(&json!({"appId": c.appid, "clientSecret": c.secret}))
      .send()
      .await?
      .value()
      .unwrap_or_default();
    match v.str("access_token") {
      Some(t) => Ok((t, v.i64("expires_in").unwrap_or(7200))),
      None => Err(
        Error::auth(format!(
          "QQ did not issue an access token: {}",
          v.str("message").unwrap_or_else(|| "no reason given".into())
        ))
        .with_hint("check the AppID and AppSecret at q.qq.com, and this machine's IP in the bot's IP whitelist"),
      ),
    }
  }

  /// Forget the token (it was refused).
  fn forget(&self, c: &Credentials) {
    self.token.forget(&self.ctx, &token_key(c));
  }

  pub async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
    self.send(Method::GET, path, query, Body::None).await
  }

  pub async fn post(&self, path: &str, body: &Value) -> Result<Value> {
    self.send(Method::POST, path, &[], Body::Json(body)).await
  }

  pub async fn put(&self, path: &str) -> Result<Value> {
    self.send(Method::PUT, path, &[], Body::None).await
  }

  pub async fn delete(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
    self.send(Method::DELETE, path, query, Body::None).await
  }

  pub async fn send(
    &self,
    method: Method,
    path: &str,
    query: &[(&str, String)],
    body: Body<'_>,
  ) -> Result<Value> {
    self.request(method, path, query, body, None).await
  }

  pub async fn post_multipart(&self, path: &str, parts: Vec<Part>) -> Result<Value> {
    self
      .request(Method::POST, path, &[], Body::None, Some(parts))
      .await
  }

  async fn request(
    &self,
    method: Method,
    path: &str,
    query: &[(&str, String)],
    body: Body<'_>,
    parts: Option<Vec<Part>>,
  ) -> Result<Value> {
    let creds = self.require()?;
    let mut refreshed = false;
    loop {
      let token = self.access_token(&creds).await?;
      let mut req = self
        .ctx
        .http
        .request(method.clone(), format!("{}{path}", Self::base(&creds)))
        .no_cookies()
        .header("authorization", format!("QQBot {token}"))
        .header("x-union-appid", &creds.appid)
        .queries(query.iter().map(|(k, v)| (*k, v)));
      if let Body::Json(v) = body {
        req = req.json(v);
      }
      if let Some(p) = &parts {
        req = req.multipart(p.clone());
      }
      let resp = req.send().await?;
      let status = resp.status.as_u16();
      let v: Value = serde_json::from_slice(&resp.body).unwrap_or_default();
      if status == 401 && !refreshed {
        self.forget(&creds);
        refreshed = true;
        continue;
      }
      let code = v.i64("code").unwrap_or(0);
      // A message held for review (304023 / 304024) was accepted.
      if (resp.status.is_success() && code == 0) || matches!(code, 304023 | 304024) {
        return Ok(v);
      }
      return Err(error(status, &v));
    }
  }
}

fn token_key(c: &Credentials) -> String {
  format!("token-{}", c.appid)
}

fn error(status: u16, v: &Value) -> Error {
  let code = v.i64("code").unwrap_or_default();
  let message = v.str("message").unwrap_or_else(|| format!("HTTP {status}"));
  let text = format!("QQ: {message} (code {code})");
  match (status, code) {
    (401, _) => Error::auth(text).with_hint("check the AppID and AppSecret"),
    (403, _) => Error::new(ErrorCode::PermissionDenied, text),
    (404, _) => Error::not_found(text),
    (429, _) | (_, 22009 | 40054005) => Error::new(ErrorCode::RateLimited, text),
    _ => Error::upstream(text),
  }
}

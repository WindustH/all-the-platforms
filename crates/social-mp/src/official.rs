//! The official API (`https://api.weixin.qq.com`) with the account's AppID
//! and AppSecret.
//!
//! Tokens come from `stable_token` in normal mode (the same token while it
//! lives, so parallel runs do not invalidate each other) and are cached until
//! shortly before they expire (`social_core::token`). Every reply carries `errcode`; the ones a
//! user can act on (IP whitelist, missing permission, quota) map to errors
//! with hints. Which account types may call what is in the guide.

use std::cell::RefCell;
use std::time::Duration;

use social_core::http::Part;
use social_core::token::AccessToken;
use social_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt, credentials, json};

use crate::api::LOGIN_HINT;

const API: &str = "https://api.weixin.qq.com";
/// Session extras holding `login --token` credentials.
const APPID_KEY: &str = "appid";
const SECRET_KEY: &str = "secret";

#[derive(Clone)]
pub struct Credentials {
  pub appid: String,
  pub secret: String,
}

/// What the route keeps between calls: the credentials and their token.
pub struct State {
  credentials: RefCell<Option<Credentials>>,
  token: AccessToken,
}

impl State {
  /// Credentials from `WECHAT_APPID` / `WECHAT_SECRET` (the names md2wechat
  /// uses; they win), else saved by `login --token APPID:SECRET`.
  pub fn new(ctx: &Ctx) -> Self {
    let from_env = credentials::env("WECHAT_APPID")
      .zip(credentials::env("WECHAT_SECRET"))
      .or_else(|| {
        let saved = credentials::saved(ctx, &[APPID_KEY, SECRET_KEY])?;
        Some((saved[0].clone(), saved[1].clone()))
      })
      .map(|(appid, secret)| Credentials { appid, secret });
    Self {
      credentials: RefCell::new(from_env),
      token: AccessToken::default(),
    }
  }
}

/// The official API as seen from one call site.
pub struct Official<'a> {
  ctx: &'a Ctx,
  state: &'a State,
}

impl<'a> Official<'a> {
  pub fn new(ctx: &'a Ctx, state: &'a State) -> Self {
    Self { ctx, state }
  }

  pub fn has(&self) -> bool {
    self.state.credentials.borrow().is_some()
  }

  pub fn credentials(&self) -> Result<Credentials> {
    self.state.credentials.borrow().clone().ok_or_else(|| {
      Error::auth("this needs the official API: no AppID / AppSecret")
        .with_hint("`social mp login --token APPID:SECRET`, or set WECHAT_APPID and WECHAT_SECRET")
    })
  }

  /// `APPID:SECRET` from `login --token`, kept in the session extras.
  pub fn set_credentials(&self, token: &str) -> Result<()> {
    let fields = credentials::fields(token);
    let [appid, secret] = fields.as_slice() else {
      return Err(credentials_form());
    };
    if !appid.starts_with("wx") || secret.is_empty() {
      return Err(credentials_form());
    }
    credentials::save(self.ctx, &[(APPID_KEY, appid), (SECRET_KEY, secret)]);
    *self.state.credentials.borrow_mut() = Some(Credentials {
      appid: appid.clone(),
      secret: secret.clone(),
    });
    self.state.token.forget(self.ctx, &token_key(appid));
    Ok(())
  }

  /// A valid access token: from memory, the cache or `stable_token`.
  async fn token(&self) -> Result<String> {
    let creds = self.credentials()?;
    let key = token_key(&creds.appid);
    self
      .state
      .token
      .get(self.ctx, &key, async || {
        stable_token(self.ctx, &creds).await
      })
      .await
  }

  /// Forget a token the API refused, so the next call fetches one.
  fn forget_token(&self) {
    if let Ok(creds) = self.credentials() {
      self.state.token.forget(self.ctx, &token_key(&creds.appid));
    }
  }

  pub async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
    self.call(path, Body::Query(query)).await
  }

  pub async fn post(&self, path: &str, body: &Value) -> Result<Value> {
    self.call(path, Body::Json(body)).await
  }

  /// Multipart upload of one file as the field `media`, with extra fields.
  pub async fn upload(
    &self,
    path: &str,
    query: &[(&str, String)],
    file: Part,
    fields: Vec<Part>,
  ) -> Result<Value> {
    let mut parts = vec![file];
    parts.extend(fields);
    self.call(path, Body::Multipart(query, parts)).await
  }

  /// One call; a refused token is fetched again once.
  async fn call(&self, path: &str, body: Body<'_>) -> Result<Value> {
    for attempt in 0..2 {
      let token = self.token().await?;
      let url = format!("{API}{path}");
      let http = &self.ctx.http;
      let req = match &body {
        Body::Query(q) => http.get(url).queries(q.iter().map(|(k, v)| (*k, v))),
        Body::Json(v) => http.post(url).json(v),
        Body::Multipart(q, parts) => http
          .post(url)
          .queries(q.iter().map(|(k, v)| (*k, v)))
          .multipart(parts.clone()),
      };
      let resp = req
        .query("access_token", &token)
        .no_cookies()
        .timeout(Duration::from_secs(120))
        .send()
        .await?;
      let v: Value = resp.value()?;
      match v.i64("errcode") {
        Some(40001 | 40014 | 42001) if attempt == 0 => self.forget_token(),
        _ => return check(v, path),
      }
    }
    unreachable!("the second attempt always returns")
  }
}

fn token_key(appid: &str) -> String {
  format!("api-token-{appid}")
}

fn credentials_form() -> Error {
  Error::input("expected `APPID:SECRET`, the AppID (wx...) and AppSecret of the account")
}

/// A token from `stable_token` in normal mode, and how long it lives.
async fn stable_token(ctx: &Ctx, creds: &Credentials) -> Result<(String, i64)> {
  let body = json!({
    "grant_type": "client_credential",
    "appid": creds.appid,
    "secret": creds.secret,
  });
  let v = ctx
    .http
    .post(format!("{API}/cgi-bin/stable_token"))
    .no_cookies()
    .json(&body)
    .value()
    .await?;
  let v = check(v, "/cgi-bin/stable_token")?;
  let token = v
    .str("access_token")
    .ok_or_else(|| Error::upstream("stable_token returned no access_token"))?;
  Ok((token, v.i64("expires_in").unwrap_or(7200)))
}

enum Body<'a> {
  Query(&'a [(&'a str, String)]),
  Json(&'a Value),
  Multipart(&'a [(&'a str, String)], Vec<Part>),
}

/// `errcode` 0 (or none) passes; everything else becomes an error with what
/// the user can do about it.
pub fn check(v: Value, path: &str) -> Result<Value> {
  let code = v.i64("errcode").unwrap_or(0);
  if code == 0 {
    return Ok(v);
  }
  let msg = v.str("errmsg").unwrap_or_default();
  let detail = format!("{path}: {code} {msg}");
  Err(match code {
    40164 | 61004 => {
      let ip = msg
        .split_whitespace()
        .find(|w| w.chars().filter(|c| *c == '.').count() == 3)
        .unwrap_or("this machine's address");
      Error::new(ErrorCode::IpBlocked, format!("IP not on the API whitelist ({detail})")).with_hint(
        format!(
          "add {ip} to the API IP whitelist: 微信开发者平台 → 我的业务 → 公众号 → 基础信息 → 开发密钥"
        ),
      )
    }
    48001 => Error::new(
      ErrorCode::PermissionDenied,
      format!("this account may not call the API ({detail})"),
    )
    .with_hint(permission_hint(path)),
    40125 | 40013 | 40002 | 41002 | 41004 | 40243 => {
      Error::auth(format!("AppID / AppSecret refused ({detail})")).with_hint(LOGIN_HINT)
    }
    45009 | 45011 | 45065 | 45066 => Error::new(ErrorCode::RateLimited, detail)
      .with_hint("daily or per-minute API quota reached; `social mp quota <api path>` shows it"),
    89503 => Error::new(
      ErrorCode::VerificationRequired,
      format!("the account admin must confirm calls from this IP ({detail})"),
    )
    .with_hint("approve the message the admin received, then retry from the same IP (or set up the IP whitelist)"),
    89506 | 89507 => Error::new(
      ErrorCode::IpBlocked,
      format!("the admin rejected calls from this IP ({detail})"),
    )
    .with_hint("wait (1 h after a first rejection, 24 h after repeated ones) or use the IP whitelist"),
    88000 => Error::new(ErrorCode::PermissionDenied, format!("no comment permission ({detail})"))
      .with_hint("comments need the 留言 feature and a verified account (仅认证)"),
    40007 | 53600 | 88001 | 88008 => Error::not_found(detail),
    61503 => Error::upstream(format!("data not ready yet ({detail})"))
      .with_hint("statistics for a day are ready after 08:00 (China time) the next day"),
    53503 | 53504 | 53505 | 48021 => Error::new(ErrorCode::PermissionDenied, detail).with_hint(
      "WeChat wants this draft handled in the mp.weixin.qq.com backend first (save it there, or pass the quiz)",
    ),
    _ => Error::upstream(detail),
  })
}

fn permission_hint(path: &str) -> &'static str {
  if path.contains("freepublish") {
    "since July 2025 publishing via API is only for verified organization accounts \
     (个人主体、未认证账号已被回收); publish in the backend instead: `social mp login` (QR)"
  } else if path.contains("draft") || path.contains("material") || path.contains("media/") {
    "drafts and material work for every account type; check the AppID / AppSecret and the API permissions"
  } else {
    "this API is only for verified organization accounts (仅认证); a QR login reaches most of it \
     through the backend: `social mp login`"
  }
}

//! Feishu / Lark Open Platform transport (`open.feishu.cn` or
//! `open.larksuite.com`, `/open-apis/...`) with an app's tenant token.
//!
//! Credentials: `login --token APP_ID:APP_SECRET` (prefix `lark:` for
//! larksuite.com), or FEISHU_APP_ID / FEISHU_APP_SECRET (LARK_APP_ID /
//! LARK_APP_SECRET for Lark), which win over the saved ones. The tenant
//! access token is cached until shortly before it expires and fetched again
//! when the API calls it invalid. Replies carry `code` (0 = fine); the ones
//! a user can act on (missing scopes, bot not in the chat, rate limits) map
//! to errors with hints.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::time::Duration;

use atp_core::credentials;
use atp_core::http::{Method, Part, Req};
use atp_core::token::AccessToken;
use atp_core::{Ctx, Error, ErrorCode, PageReq, Result, Value, ValueExt, json};

pub const FEISHU: &str = "https://open.feishu.cn";
pub const LARK: &str = "https://open.larksuite.com";
const APP_ID: &str = "app_id";
const APP_SECRET: &str = "app_secret";
const BASE: &str = "base";
/// Codes that mean the tenant token is invalid or expired.
const STALE_TOKEN: [i64; 4] = [99991661, 99991663, 99991664, 99991665];
pub const LOGIN_HINT: &str = "run `atp feishu login --token APP_ID:APP_SECRET` (`lark:APP_ID:APP_SECRET` \
   for larksuite.com), or set FEISHU_APP_ID and FEISHU_APP_SECRET";

/// An app's credentials and the host they belong to.
#[derive(Debug, Clone)]
pub struct Creds {
  pub app_id: String,
  pub secret: String,
  pub base: String,
}

impl Creds {
  /// `APP_ID:APP_SECRET`, optionally behind `lark:` / `feishu:`.
  pub fn parse(raw: &str) -> Result<Self> {
    let fields = credentials::fields(raw);
    let fields: Vec<&str> = fields.iter().map(String::as_str).collect();
    let (base, rest) = match fields.as_slice() {
      ["lark" | "larksuite", rest @ ..] => (LARK, rest),
      ["feishu", rest @ ..] => (FEISHU, rest),
      rest => (FEISHU, rest),
    };
    match rest {
      [id, secret] if id.starts_with("cli_") && !secret.is_empty() => Ok(Self {
        app_id: (*id).to_owned(),
        secret: (*secret).to_owned(),
        base: base.to_owned(),
      }),
      _ => Err(
        Error::input("expected APP_ID:APP_SECRET (the app id starts with `cli_`)")
          .with_hint(LOGIN_HINT),
      ),
    }
  }

  /// FEISHU_APP_ID / FEISHU_APP_SECRET (with FEISHU_BASE_URL), else
  /// LARK_APP_ID / LARK_APP_SECRET.
  fn from_env() -> Option<Self> {
    let feishu = (
      credentials::env("FEISHU_APP_ID"),
      credentials::env("FEISHU_APP_SECRET"),
    );
    if let (Some(app_id), Some(secret)) = feishu {
      let base = credentials::env("FEISHU_BASE_URL").unwrap_or_else(|| FEISHU.to_owned());
      return Some(Self {
        app_id,
        secret,
        base,
      });
    }
    let lark = (
      credentials::env("LARK_APP_ID"),
      credentials::env("LARK_APP_SECRET"),
    );
    let (Some(app_id), Some(secret)) = lark else {
      return None;
    };
    Some(Self {
      app_id,
      secret,
      base: LARK.to_owned(),
    })
  }
}

/// What a request carries.
pub enum Body {
  None,
  Json(Value),
  Multipart(Vec<Part>),
}

pub struct Api {
  pub ctx: Ctx,
  env: Option<Creds>,
  token: AccessToken,
  /// Display names of users and chats seen this run (open id / chat id → name).
  pub names: RefCell<HashMap<String, String>>,
  /// The contact API refused once: do not ask again this run.
  pub contacts_denied: Cell<bool>,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self {
      ctx,
      env: Creds::from_env(),
      token: AccessToken::default(),
      names: RefCell::new(HashMap::new()),
      contacts_denied: Cell::new(false),
    }
  }

  pub fn creds(&self) -> Option<Creds> {
    if let Some(c) = &self.env {
      return Some(c.clone());
    }
    let [app_id, secret] = credentials::saved(&self.ctx, &[APP_ID, APP_SECRET])?
      .try_into()
      .ok()?;
    Some(Creds {
      app_id,
      secret,
      base: self.ctx.extra(BASE).unwrap_or_else(|| FEISHU.to_owned()),
    })
  }

  pub fn logged_in(&self) -> bool {
    self.creds().is_some()
  }

  pub fn require(&self) -> Result<Creds> {
    self
      .creds()
      .ok_or_else(|| Error::auth("no Feishu app credentials").with_hint(LOGIN_HINT))
  }

  pub fn save(&self, creds: &Creds) {
    credentials::save(
      &self.ctx,
      &[
        (APP_ID, &creds.app_id),
        (APP_SECRET, &creds.secret),
        (BASE, &creds.base),
      ],
    );
  }

  /// A tenant access token: from memory, the cache, or a new one.
  pub async fn token(&self) -> Result<String> {
    let creds = self.require()?;
    self
      .token
      .get(&self.ctx, &token_key(&creds), async || {
        fetch_token(&self.ctx, &creds).await
      })
      .await
  }

  fn forget_token(&self) {
    if let Some(creds) = self.creds() {
      self.token.forget(&self.ctx, &token_key(&creds));
    }
  }

  /// `Authorization` header value, for downloads of message resources.
  pub async fn bearer(&self) -> Result<String> {
    Ok(format!("Bearer {}", self.token().await?))
  }

  pub fn base(&self) -> String {
    self
      .creds()
      .map(|c| c.base)
      .unwrap_or_else(|| FEISHU.to_owned())
  }

  pub async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
    self.call(Method::GET, path, query, Body::None).await
  }

  pub async fn post(&self, path: &str, query: &[(&str, String)], body: Value) -> Result<Value> {
    self.call(Method::POST, path, query, Body::Json(body)).await
  }

  pub async fn put(&self, path: &str, body: Value) -> Result<Value> {
    self.call(Method::PUT, path, &[], Body::Json(body)).await
  }

  pub async fn delete(&self, path: &str) -> Result<Value> {
    self.call(Method::DELETE, path, &[], Body::None).await
  }

  pub async fn upload(&self, path: &str, parts: Vec<Part>) -> Result<Value> {
    self
      .call(Method::POST, path, &[], Body::Multipart(parts))
      .await
  }

  /// One API call; a token the API calls invalid is renewed once.
  pub async fn call(
    &self,
    method: Method,
    path: &str,
    query: &[(&str, String)],
    body: Body,
  ) -> Result<Value> {
    for attempt in 0..2 {
      let url = format!("{}/open-apis{path}", self.base());
      let mut req: Req<'_> = self
        .ctx
        .http
        .request(method.clone(), url)
        .no_cookies()
        .header("authorization", self.bearer().await?)
        .queries(query.iter().map(|(k, v)| (*k, v)));
      req = match &body {
        Body::None => req,
        Body::Json(v) => req.json(v),
        Body::Multipart(parts) => req
          .multipart(parts.clone())
          .timeout(Duration::from_secs(300)),
      };
      let resp = req.send().await?;
      let v: Value = resp
        .value()
        .map_err(|e| Error::upstream(format!("{path}: HTTP {} ({})", resp.status, e.message)))?;
      match v.i64("code") {
        Some(code) if STALE_TOKEN.contains(&code) && attempt == 0 => self.forget_token(),
        _ => return check(v, path),
      }
    }
    unreachable!("the second attempt always returns")
  }
}

fn token_key(creds: &Creds) -> String {
  format!("tenant-token-{}", creds.app_id)
}

async fn fetch_token(ctx: &Ctx, creds: &Creds) -> Result<(String, i64)> {
  let v: Value = ctx
    .http
    .post(format!(
      "{}/open-apis/auth/v3/tenant_access_token/internal",
      creds.base
    ))
    .no_cookies()
    .json(&json!({ "app_id": creds.app_id, "app_secret": creds.secret }))
    .value()
    .await?;
  let v = check(v, "/auth/v3/tenant_access_token/internal")?;
  let token = v
    .str("tenant_access_token")
    .ok_or_else(|| Error::upstream("no tenant_access_token in the reply"))?;
  Ok((token, v.i64("expire").unwrap_or(7200)))
}

/// `query` with one page's `page_size` (at most `max`) and `page_token`.
pub fn page_query<'a>(
  mut query: Vec<(&'a str, String)>,
  req: &PageReq,
  max: usize,
) -> Vec<(&'a str, String)> {
  query.push(("page_size", req.size_within(max).to_string()));
  if let Some(cursor) = &req.cursor {
    query.push(("page_token", cursor.clone()));
  }
  query
}

/// The cursor of the next page, when there is one.
pub fn next_page(v: &Value) -> Option<String> {
  (v.bool("data.has_more") == Some(true))
    .then(|| v.str("data.page_token"))
    .flatten()
}

/// `code` 0 passes; the rest become errors saying what to do.
pub fn check(v: Value, path: &str) -> Result<Value> {
  let code = v.i64("code").unwrap_or(0);
  if code == 0 {
    return Ok(v);
  }
  let msg = v.str("msg").unwrap_or_default();
  let detail = format!("{path}: {code} {msg}");
  Err(match code {
    10003 | 10014 | 10015 | 99991661 | 99991663 | 99991664 | 99991665 | 1000040344 | 1000040345
    | 1000040346 | 1000040347 => {
      Error::auth(format!("Feishu refused the app credentials ({detail})")).with_hint(LOGIN_HINT)
    }
    99991672 | 99991679 | 99991676 | 230027 | 232025 => Error::new(
      ErrorCode::PermissionDenied,
      format!("the app lacks a permission ({detail})"),
    )
    .with_hint(
      "add the scope named above in the developer console (权限管理), publish a new app version, and retry",
    ),
    230002 | 232011 | 232033 => Error::new(
      ErrorCode::PermissionDenied,
      format!("the bot is not in this chat ({detail})"),
    )
    .with_hint("add the bot to the chat first"),
    230006 => Error::new(
      ErrorCode::PermissionDenied,
      format!("the app has no bot ability ({detail})"),
    )
    .with_hint("enable 机器人 (Bot) under 应用能力 in the developer console"),
    230001 | 99992402 | 1000 | 230099 => Error::input(detail),
    99991400 | 99991401 | 230020 => Error::new(ErrorCode::RateLimited, detail),
    1000040350 => Error::new(
      ErrorCode::RateLimited,
      format!("too many long connections for this app ({detail})"),
    )
    .with_hint("close other `listen` runs or SDK clients of the same app"),
    230011 | 231003 | 230110 => Error::not_found(detail),
    _ => Error::upstream(detail),
  })
}

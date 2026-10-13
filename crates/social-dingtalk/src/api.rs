//! DingTalk transport: the current API (`api.dingtalk.com/v1.0`, token in
//! `x-acs-dingtalk-access-token`, errors as `{code, message}`) and the older
//! one (`oapi.dingtalk.com`, `?access_token=`, `errcode`), with an internal
//! app's AppKey and AppSecret.
//!
//! Credentials: `login --token APPKEY:APPSECRET[:ROBOTCODE]`, or
//! DINGTALK_APP_KEY / DINGTALK_APP_SECRET (DINGTALK_CLIENT_ID /
//! DINGTALK_CLIENT_SECRET) and DINGTALK_ROBOT_CODE, which win over the saved
//! ones. The robot code of an internal app's bot is usually its AppKey.

use std::cell::RefCell;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use social_core::http::{Method, Part};
use social_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt, json};

pub const API: &str = "https://api.dingtalk.com";
pub const OAPI: &str = "https://oapi.dingtalk.com";
const KEY: &str = "app_key";
const SECRET: &str = "app_secret";
const ROBOT: &str = "robot_code";
const MARGIN_SECS: i64 = 300;
pub const LOGIN_HINT: &str = "run `social dingtalk login --token APPKEY:APPSECRET[:ROBOTCODE]`, or set \
   DINGTALK_APP_KEY and DINGTALK_APP_SECRET; group robot webhooks need no login";

#[derive(Debug, Clone)]
pub struct Creds {
  pub app_key: String,
  pub secret: String,
  pub robot_code: String,
}

impl Creds {
  pub fn parse(raw: &str) -> Result<Self> {
    let mut parts = raw.trim().splitn(3, ':');
    match (parts.next(), parts.next(), parts.next()) {
      (Some(key), Some(secret), robot) if !key.is_empty() && !secret.is_empty() => Ok(Self {
        app_key: key.to_owned(),
        secret: secret.to_owned(),
        robot_code: robot.filter(|r| !r.is_empty()).unwrap_or(key).to_owned(),
      }),
      _ => Err(Error::input("expected APPKEY:APPSECRET[:ROBOTCODE]").with_hint(LOGIN_HINT)),
    }
  }
}

#[derive(Serialize, Deserialize)]
struct Cached {
  token: String,
  expires: i64,
}

pub struct Api {
  pub ctx: Ctx,
  env: Option<Creds>,
  token: RefCell<Option<String>>,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    let var = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
    let key = var("DINGTALK_APP_KEY").or_else(|| var("DINGTALK_CLIENT_ID"));
    let secret = var("DINGTALK_APP_SECRET").or_else(|| var("DINGTALK_CLIENT_SECRET"));
    let env = match (key, secret) {
      (Some(app_key), Some(secret)) => Some(Creds {
        robot_code: var("DINGTALK_ROBOT_CODE").unwrap_or_else(|| app_key.clone()),
        app_key,
        secret,
      }),
      _ => None,
    };
    Self {
      ctx,
      env,
      token: RefCell::new(None),
    }
  }

  pub fn creds(&self) -> Option<Creds> {
    if let Some(c) = &self.env {
      return Some(c.clone());
    }
    let app_key = self.ctx.extra(KEY).filter(|v| !v.is_empty())?;
    Some(Creds {
      secret: self.ctx.extra(SECRET).filter(|v| !v.is_empty())?,
      robot_code: self
        .ctx
        .extra(ROBOT)
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| app_key.clone()),
      app_key,
    })
  }

  pub fn require(&self) -> Result<Creds> {
    self
      .creds()
      .ok_or_else(|| Error::auth("no DingTalk app credentials").with_hint(LOGIN_HINT))
  }

  pub fn save(&self, creds: &Creds) {
    self.ctx.set_extra(KEY, &creds.app_key);
    self.ctx.set_extra(SECRET, &creds.secret);
    self.ctx.set_extra(ROBOT, &creds.robot_code);
    *self.token.borrow_mut() = None;
  }

  fn cache_key(creds: &Creds) -> String {
    format!("token-{}", creds.app_key)
  }

  pub async fn token(&self) -> Result<String> {
    if let Some(t) = self.token.borrow().clone() {
      return Ok(t);
    }
    let creds = self.require()?;
    let now = jiff::Timestamp::now().as_second();
    let cached = self
      .ctx
      .store
      .cache_get::<Cached>(&Self::cache_key(&creds), Duration::from_secs(7200))
      .filter(|c| c.expires - MARGIN_SECS > now && !c.token.is_empty());
    let token = match cached {
      Some(c) => c.token,
      None => {
        let resp = self
          .ctx
          .http
          .post(format!("{API}/v1.0/oauth2/accessToken"))
          .no_cookies()
          .json(&json!({ "appKey": creds.app_key, "appSecret": creds.secret }))
          .send()
          .await?;
        let v = check_api(
          resp.status.as_u16(),
          resp.value().unwrap_or_default(),
          "accessToken",
        )?;
        let token = v
          .str("accessToken")
          .ok_or_else(|| Error::upstream("no accessToken in the reply"))?;
        self.ctx.store.cache_put(
          &Self::cache_key(&creds),
          &Cached {
            token: token.clone(),
            expires: now + v.i64("expireIn").unwrap_or(7200),
          },
        );
        token
      }
    };
    *self.token.borrow_mut() = Some(token.clone());
    Ok(token)
  }

  fn forget_token(&self) {
    *self.token.borrow_mut() = None;
    if let Some(creds) = self.creds() {
      self.ctx.store.cache_put(
        &Self::cache_key(&creds),
        &Cached {
          token: String::new(),
          expires: 0,
        },
      );
    }
  }

  /// The current API (`/v1.0/...`).
  pub async fn api(
    &self,
    method: Method,
    path: &str,
    query: &[(&str, String)],
    body: Option<Value>,
  ) -> Result<Value> {
    for attempt in 0..2 {
      let mut req = self
        .ctx
        .http
        .request(method.clone(), format!("{API}{path}"))
        .no_cookies()
        .header("x-acs-dingtalk-access-token", self.token().await?)
        .queries(query.iter().map(|(k, v)| (*k, v)));
      if let Some(b) = &body {
        req = req.json(b);
      }
      let resp = req.send().await?;
      let status = resp.status.as_u16();
      let v = resp.value().unwrap_or_default();
      let invalid = v
        .str("code")
        .is_some_and(|c| c.contains("AccessTokenInvalid") || c == "InvalidAuthentication");
      if invalid && attempt == 0 {
        self.forget_token();
        continue;
      }
      return check_api(status, v, path);
    }
    unreachable!("the second attempt always returns")
  }

  pub async fn post(&self, path: &str, body: Value) -> Result<Value> {
    self.api(Method::POST, path, &[], Some(body)).await
  }

  /// The older API (`oapi.dingtalk.com`), JSON body or multipart.
  pub async fn oapi(&self, path: &str, query: &[(&str, String)], body: OapiBody) -> Result<Value> {
    for attempt in 0..2 {
      let token = self.token().await?;
      let mut req = self
        .ctx
        .http
        .post(format!("{OAPI}{path}"))
        .no_cookies()
        .query("access_token", &token)
        .queries(query.iter().map(|(k, v)| (*k, v)));
      req = match &body {
        OapiBody::Json(v) => req.json(v),
        OapiBody::Multipart(parts) => req
          .multipart(parts.clone())
          .timeout(Duration::from_secs(300)),
      };
      let v: Value = req.send().await?.value()?;
      match v.i64("errcode") {
        Some(40001 | 40014 | 42001) if attempt == 0 => self.forget_token(),
        _ => return check_oapi(v, path),
      }
    }
    unreachable!("the second attempt always returns")
  }
}

pub enum OapiBody {
  Json(Value),
  Multipart(Vec<Part>),
}

/// Errors of the current API: non-2xx with `{code, message}`.
pub fn check_api(status: u16, v: Value, path: &str) -> Result<Value> {
  let code = v.str("code");
  if (200..300).contains(&status) && code.is_none() {
    return Ok(v);
  }
  let code = code.unwrap_or_else(|| format!("HTTP {status}"));
  let detail = format!("{path}: {code} {}", v.str("message").unwrap_or_default());
  Err(
    if code.contains("AccessTokenInvalid")
      || code == "InvalidAuthentication"
      || code.contains("authFailed")
      || code == "invalidClientIdOrSecret"
    {
      Error::auth(format!("DingTalk refused the app credentials ({detail})")).with_hint(LOGIN_HINT)
    } else if code.starts_with("Forbidden") {
      Error::new(
        ErrorCode::PermissionDenied,
        format!("the app lacks a permission ({detail})"),
      )
      .with_hint("grant the permission (权限管理) to the app in the DingTalk developer console")
    } else if code.starts_with("Throttling") {
      Error::new(ErrorCode::RateLimited, detail)
    } else if code.starts_with("InvalidParameter")
      || code.starts_with("MissingParameter")
      || code.starts_with("param")
    {
      Error::input(detail)
    } else if code.contains("NotFound") || code.contains("notFound") || code.contains("not.found") {
      Error::not_found(detail)
    } else {
      Error::upstream(detail)
    },
  )
}

/// Errors of the older API: `errcode` other than 0.
pub fn check_oapi(v: Value, path: &str) -> Result<Value> {
  let code = v.i64("errcode").unwrap_or(0);
  if code == 0 {
    return Ok(v);
  }
  let detail = format!("{path}: {code} {}", v.str("errmsg").unwrap_or_default());
  Err(match code {
    40001 | 40014 | 42001 | 40089 => Error::auth(detail).with_hint(LOGIN_HINT),
    60011 | 60020 | 60121 | 40078 => Error::new(ErrorCode::PermissionDenied, detail)
      .with_hint("grant the permission to the app in the DingTalk developer console"),
    88 | 90002 | 90006 | 90018 | 130101 | 410100 => Error::new(ErrorCode::RateLimited, detail),
    300001 | 310000 => Error::new(ErrorCode::PermissionDenied, detail)
      .with_hint("a webhook with 加签 needs its secret: append `&secret=SEC…` to the URL"),
    33012 | 300005 => Error::not_found(detail),
    _ => Error::upstream(detail),
  })
}

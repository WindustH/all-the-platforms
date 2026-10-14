//! The POIZON Open Platform (`open.poizon.com`), Dewu's official API: one
//! JSON POST per call with the business fields, `app_key`, `timestamp`
//! (ms) and `sign` ([`sign::open`]). The catalogue, price and sales calls
//! need the app signature only (no OAuth token), from an address on the
//! app's IP whitelist. Credentials come from `DEWU_APP_KEY` /
//! `DEWU_APP_SECRET` (they win) or `login --token APP_KEY:APP_SECRET`,
//! kept in the session.

use atp_core::credentials;
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt};
use serde_json::Map;

use crate::sign;

const GATEWAY: &str = "https://open.poizon.com";
const KEYS: [&str; 2] = ["app_key", "app_secret"];
pub const LOGIN_HINT: &str = "run `atp dewu login --token APP_KEY:APP_SECRET` with an app of the \
  POIZON Open Platform (https://open.poizon.com), or set DEWU_APP_KEY and DEWU_APP_SECRET";

/// An Open Platform app.
#[derive(Clone)]
pub struct Creds {
  pub key: String,
  secret: String,
}

pub struct Open {
  env: Option<Creds>,
}

impl Open {
  pub fn from_env() -> Self {
    let env = match (
      credentials::env("DEWU_APP_KEY"),
      credentials::env("DEWU_APP_SECRET"),
    ) {
      (Some(key), Some(secret)) => Some(Creds { key, secret }),
      _ => None,
    };
    Self { env }
  }

  /// The environment's credentials, else the saved ones.
  pub fn creds(&self, ctx: &Ctx) -> Option<Creds> {
    if let Some(c) = &self.env {
      return Some(c.clone());
    }
    let saved = credentials::saved(ctx, &KEYS)?;
    Some(Creds {
      key: saved[0].clone(),
      secret: saved[1].clone(),
    })
  }

  pub fn has(&self, ctx: &Ctx) -> bool {
    self.creds(ctx).is_some()
  }

  pub fn require(&self, ctx: &Ctx) -> Result<Creds> {
    self
      .creds(ctx)
      .ok_or_else(|| Error::auth("no Dewu Open Platform app").with_hint(LOGIN_HINT))
  }

  /// Keep `APP_KEY:APP_SECRET` in the session.
  pub fn save(ctx: &Ctx, token: &str) -> Result<()> {
    let fields = credentials::expect(token, 2, 2, "APP_KEY:APP_SECRET")?;
    credentials::save(ctx, &[(KEYS[0], &fields[0]), (KEYS[1], &fields[1])]);
    Ok(())
  }

  /// One call (`path` below the gateway, `/dop/api/v1/...`); its `data`.
  pub async fn call(&self, ctx: &Ctx, path: &str, biz: Value) -> Result<Value> {
    let creds = self.require(ctx)?;
    let mut params = match biz {
      Value::Object(map) => map,
      _ => Map::new(),
    };
    params.insert("app_key".into(), creds.key.into());
    params.insert(
      "timestamp".into(),
      jiff::Timestamp::now().as_millisecond().into(),
    );
    let sign = sign::open(&params, &creds.secret);
    params.insert("sign".into(), sign.into());
    let resp = ctx
      .http
      .post(format!("{GATEWAY}{path}"))
      .no_cookies()
      .json(&params)
      .send()
      .await?;
    let v = resp
      .value()
      .map_err(|_| atp_core::http::status_error(resp.status, &resp.text()))?;
    if v.i64("code") == Some(200) {
      return Ok(v.at("data").clone());
    }
    Err(error(&v, path))
  }
}

/// The atp error for a failed answer, by its code and wording.
fn error(v: &Value, path: &str) -> Error {
  let code = v.i64("code").unwrap_or_default();
  let text = [v.str("msg"), v.str("errors.0.message")]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(": ");
  let call = path.rsplit('/').next().unwrap_or(path);
  let message = format!("Dewu Open Platform {call}: {text} (code {code})");
  let has = |words: &[&str]| words.iter().any(|w| text.contains(w));
  if code == 5025 || has(&["白名单", "whitelist"]) {
    Error::new(ErrorCode::PermissionDenied, message)
      .with_hint("add this machine's public IP to the app's IP whitelist (应用详情 → IP白名单)")
  } else if code == 401 || has(&["签名", "sign"]) {
    Error::new(ErrorCode::SignatureError, message)
      .with_hint("check the app secret given to `login --token` or DEWU_APP_SECRET")
  } else if has(&["app_key", "appKey", "应用不存在", "应用已"]) {
    Error::auth(message).with_hint(LOGIN_HINT)
  } else if code == 30300014 || has(&["频繁", "限流", "too frequent"]) {
    Error::new(ErrorCode::RateLimited, message)
  } else if has(&["无权", "权限", "permission"]) {
    Error::new(ErrorCode::PermissionDenied, message)
      .with_hint("the app lacks this API; apply for its permission package in the console")
  } else if has(&["参数", "为空或错误", "invalid"]) {
    Error::input(message)
  } else {
    Error::upstream(message)
  }
}

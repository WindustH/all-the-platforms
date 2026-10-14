//! The 美团联盟 (CPS) open API, `media.meituan.com/cps_open/common/api/v1/
//! <method>`: one JSON POST per call, signed ([`crate::sign`]) with the
//! media account's app key and secret and sent with the key, a millisecond
//! timestamp (valid two minutes) and the body's MD5. Answers carry `code`
//! (0 for success) and `message`. Credentials come from `MEITUAN_APP_KEY`
//! and `MEITUAN_APP_SECRET` (they win) or `login --token
//! APP_KEY:APP_SECRET`, kept in the session.
//!
//! The request and answer shapes follow the official SDKs; atp had no
//! credentials to try them with.

use atp_core::credentials;
use atp_core::http::{Resp, status_error};
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt};

use crate::sign;

const BASE: &str = "https://media.meituan.com";
const PATH: &str = "/cps_open/common/api/v1/";
const KEYS: [&str; 2] = ["cps_app_key", "cps_app_secret"];
pub const LOGIN_HINT: &str = "run `atp mt login --token APP_KEY:APP_SECRET` with the key and \
  secret of your 美团联盟 media account (media.meituan.com, 我的 → API 接口), or set \
  MEITUAN_APP_KEY and MEITUAN_APP_SECRET";

/// A media account's API credentials.
#[derive(Clone)]
pub struct Creds {
  pub key: String,
  secret: String,
}

pub struct Cps {
  env: Option<Creds>,
}

impl Cps {
  pub fn from_env() -> Self {
    let env = match (
      credentials::env("MEITUAN_APP_KEY"),
      credentials::env("MEITUAN_APP_SECRET"),
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

  /// Keep `APP_KEY:APP_SECRET` in the session.
  pub fn save(ctx: &Ctx, token: &str) -> Result<()> {
    let fields = credentials::expect(token, 2, 2, "APP_KEY:APP_SECRET")?;
    credentials::save(ctx, &[(KEYS[0], &fields[0]), (KEYS[1], &fields[1])]);
    Ok(())
  }

  /// One call of `method` (`query_coupon`, `query_order` …) with `body`.
  pub async fn call(&self, ctx: &Ctx, method: &str, body: &Value) -> Result<Value> {
    let creds = self
      .creds(ctx)
      .ok_or_else(|| Error::auth("no 美团联盟 API credentials").with_hint(LOGIN_HINT))?;
    let path = format!("{PATH}{method}");
    let text = body.to_string();
    let md5 = sign::content_md5(text.as_bytes());
    let timestamp = jiff::Timestamp::now().as_millisecond().to_string();
    let signature = sign::signature(&creds.secret, &md5, &creds.key, &timestamp, &path);
    let resp = ctx
      .http
      .post(format!("{BASE}{path}"))
      .no_cookies()
      .header("content-type", "application/json")
      .header("S-Ca-App", &creds.key)
      .header("S-Ca-Timestamp", &timestamp)
      .header("S-Ca-Signature", signature)
      .header("S-Ca-Signature-Headers", sign::SIGNED_HEADERS)
      .header("Content-MD5", md5)
      .json_text(text)
      .send()
      .await?;
    check(resp, method)
  }
}

fn check(resp: Resp, method: &str) -> Result<Value> {
  let v = match resp.value() {
    Ok(v) => v,
    Err(_) if matches!(resp.status.as_u16(), 401 | 403) => {
      return Err(
        Error::auth(format!(
          "美团联盟 refused the credentials (HTTP {})",
          resp.status
        ))
        .with_hint(LOGIN_HINT),
      );
    }
    Err(e) if resp.status.is_success() => return Err(e),
    Err(_) => return Err(status_error(resp.status, &resp.text())),
  };
  match v.i64("code").unwrap_or(0) {
    0 => Ok(v),
    code => Err(error(&v, method, code)),
  }
}

/// The atp error for a failed answer, by its wording.
fn error(v: &Value, method: &str, code: i64) -> Error {
  let text = v
    .first_str(&["message", "Message", "msg"])
    .unwrap_or_else(|| "request failed".into());
  let message = format!("美团联盟 {method}: {text} (code {code})");
  let lower = text.to_ascii_lowercase();
  let has = |words: &[&str]| words.iter().any(|w| text.contains(w) || lower.contains(w));
  if has(&["时间戳", "timestamp", "过期"]) {
    Error::new(ErrorCode::SignatureError, message)
      .with_hint("check this machine's clock: a request is valid for two minutes")
  } else if has(&["签名", "signature", "sign"]) {
    Error::new(ErrorCode::SignatureError, message)
      .with_hint("check the app secret given to `login --token` or MEITUAN_APP_SECRET")
  } else if has(&["appkey", "app_key", "鉴权", "认证", "未授权", "s-ca-app"]) {
    Error::auth(message).with_hint(LOGIN_HINT)
  } else if has(&["频", "限流", "qps", "流量"]) {
    Error::new(ErrorCode::RateLimited, message)
  } else if has(&["权限", "未开通", "无权"]) {
    Error::new(ErrorCode::PermissionDenied, message)
  } else if has(&["参数", "param"]) {
    Error::input(message)
  } else {
    Error::upstream(message)
  }
}

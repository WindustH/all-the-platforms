//! The 多多进宝 (DDK) open API, `gw-api.pinduoduo.com/api/router`: one
//! form POST per call with `type`, `client_id`, `timestamp`, `data_type`
//! and the business parameters, signed with the upper-case MD5 of
//! `secret + k1v1k2v2… + secret` over the parameters sorted by name. List
//! parameters go as JSON. Credentials come from `PDD_CLIENT_ID`,
//! `PDD_CLIENT_SECRET` and `PDD_PID` (they win) or `login --token
//! CLIENT_ID:CLIENT_SECRET:PID`, kept in the session.

use atp_core::credentials;
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt};
use md5::{Digest, Md5};

const ROUTER: &str = "https://gw-api.pinduoduo.com/api/router";
const KEYS: [&str; 3] = ["client_id", "client_secret", "pid"];
pub const LOGIN_HINT: &str = "run `atp pdd login --token CLIENT_ID:CLIENT_SECRET:PID` (a 多多进宝 \
  app from https://open.pinduoduo.com and a pid from https://jinbao.pinduoduo.com), or set \
  PDD_CLIENT_ID, PDD_CLIENT_SECRET and PDD_PID";

/// A DDK app and its promotion slot.
#[derive(Clone)]
pub struct Creds {
  pub client_id: String,
  secret: String,
  pub pid: String,
}

pub struct Ddk {
  env: Option<Creds>,
}

impl Ddk {
  pub fn from_env() -> Self {
    let env = match (
      credentials::env("PDD_CLIENT_ID"),
      credentials::env("PDD_CLIENT_SECRET"),
    ) {
      (Some(client_id), Some(secret)) => Some(Creds {
        client_id,
        secret,
        pid: credentials::env("PDD_PID").unwrap_or_default(),
      }),
      _ => None,
    };
    Self { env }
  }

  /// The environment's credentials, else the saved ones.
  pub fn creds(&self, ctx: &Ctx) -> Option<Creds> {
    if let Some(c) = &self.env {
      return Some(c.clone());
    }
    let saved = credentials::saved(ctx, &KEYS[..2])?;
    Some(Creds {
      client_id: saved[0].clone(),
      secret: saved[1].clone(),
      pid: ctx.extra(KEYS[2]).unwrap_or_default(),
    })
  }

  pub fn has(&self, ctx: &Ctx) -> bool {
    self.creds(ctx).is_some()
  }

  /// Keep `CLIENT_ID:CLIENT_SECRET:PID` in the session.
  pub fn save(ctx: &Ctx, token: &str) -> Result<()> {
    let fields = credentials::expect(token, 3, 3, "CLIENT_ID:CLIENT_SECRET:PID")?;
    let pairs: Vec<(&str, &str)> = KEYS
      .iter()
      .copied()
      .zip(fields.iter().map(String::as_str))
      .collect();
    credentials::save(ctx, &pairs);
    Ok(())
  }

  /// One API call; `method` is the `type` (`pdd.ddk.goods.search`). The
  /// answer's `<method>_response`-style object is left to the caller.
  pub async fn call(&self, ctx: &Ctx, method: &str, params: &[(&str, String)]) -> Result<Value> {
    let creds = self
      .creds(ctx)
      .ok_or_else(|| Error::auth("no 多多进宝 credentials").with_hint(LOGIN_HINT))?;
    let mut form: Vec<(String, String)> = params
      .iter()
      .filter(|(_, v)| !v.is_empty())
      .map(|(k, v)| ((*k).to_owned(), v.clone()))
      .collect();
    form.extend([
      ("type".to_owned(), method.to_owned()),
      ("client_id".to_owned(), creds.client_id.clone()),
      (
        "timestamp".to_owned(),
        jiff::Timestamp::now().as_second().to_string(),
      ),
      ("data_type".to_owned(), "JSON".to_owned()),
    ]);
    let sign = sign(&form, &creds.secret);
    form.push(("sign".to_owned(), sign));
    let resp = ctx
      .http
      .post(ROUTER)
      .no_cookies()
      .no_throttle()
      .form(form)
      .send()
      .await?;
    let v = resp.check()?.value()?;
    match v.get("error_response") {
      Some(e) => Err(error(e, method)),
      None => Ok(v),
    }
  }
}

/// Upper-case MD5 of `secret`, the sorted `name value` pairs, `secret`.
pub fn sign(params: &[(String, String)], secret: &str) -> String {
  let mut sorted: Vec<&(String, String)> = params.iter().collect();
  sorted.sort_by(|a, b| a.0.cmp(&b.0));
  let mut text = secret.to_owned();
  for (k, v) in sorted {
    text.push_str(k);
    text.push_str(v);
  }
  text.push_str(secret);
  format!("{:X}", Md5::digest(text.as_bytes()))
}

/// The atp error for an `error_response`, by its wording.
fn error(e: &Value, method: &str) -> Error {
  let code = e.i64("error_code").unwrap_or_default();
  let mut parts: Vec<String> = [e.str("error_msg"), e.str("sub_msg")]
    .into_iter()
    .flatten()
    .collect();
  parts.dedup();
  let text = parts.join(": ");
  let message = format!("多多进宝 {method}: {text} (code {code})");
  let has = |words: &[&str]| words.iter().any(|w| text.contains(w));
  if has(&["签名", "sign"]) {
    Error::new(ErrorCode::SignatureError, message)
      .with_hint("check the client secret given to `login --token` or PDD_CLIENT_SECRET")
  } else if code == 10016 || has(&["client_id", "clientId", "client下线", "应用不存在"]) {
    Error::auth(message).with_hint(LOGIN_HINT)
  } else if has(&["频", "限流", "流量"]) {
    Error::new(ErrorCode::RateLimited, message)
  } else if has(&["备案", "权限", "授权", "未开通"]) {
    Error::new(ErrorCode::PermissionDenied, message).with_hint(
      "the app or its pid lacks this permission; authorize (备案) the pid on jinbao.pinduoduo.com first",
    )
  } else if has(&["不存在", "下架", "已失效"]) {
    Error::not_found(message)
  } else if has(&["参数"]) {
    Error::input(message)
  } else {
    Error::upstream(message)
  }
}

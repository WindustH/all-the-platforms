//! Quark transport: the web drive's calls on `drive-pc.quark.cn`, each with
//! `pr=ucpro&fr=pc` and the session cookie, answering
//! `{status, code, message, data, metadata}` (`code` 0 on success); and the
//! account page on `pan.quark.cn`, answering `{success, code: "OK", data}`.
//!
//! Every response may rotate `__puus`, which the cookie jar keeps. Refusals
//! map onto atp errors: a dead session (`31001`, HTTP 401) is
//! `not_authenticated`, a risk check `verification_required`, a full drive
//! or a size limit `permission_denied`.

use atp_core::http::{Req, Resp, status_error};
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt, task};

pub const PAN: &str = "https://pan.quark.cn";
pub const REFERER: &str = "https://pan.quark.cn/";
const DRIVE: &str = "https://drive-pc.quark.cn/1/clouddrive";
/// The Quark PC client's User-Agent, sent on download requests only: the
/// drive refuses links to big files to browsers (`23018`).
pub const CLIENT_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
  (KHTML, like Gecko) quark-cloud-drive/2.5.56 Chrome/100.0.4896.160 \
  Electron/18.3.5.12-a038f7b798 Safari/537.36 Channel/pckk_other_ch";
/// Calls a rate limit turns away get this many tries in all.
const TRIES: u32 = 3;
const RISK_HINT: &str = "Quark wants a check in the web drive: open https://pan.quark.cn, \
  pass it, then try again";

pub struct Api {
  pub ctx: Ctx,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self { ctx }
  }

  /// A GET of the drive (`path` under `/1/clouddrive`); the whole answer.
  pub async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value> {
    task::retry(TRIES, rate_limited, async || {
      let req = self.drive(self.ctx.http.get(format!("{DRIVE}{path}")));
      self.check(req.queries(query.iter().cloned()).send().await?, path)
    })
    .await
  }

  /// A JSON POST of the drive; the whole answer.
  pub async fn post(&self, path: &str, body: &Value) -> Result<Value> {
    self.post_as(path, body, None).await
  }

  /// [`post`](Self::post) with another User-Agent.
  pub async fn post_as(&self, path: &str, body: &Value, agent: Option<&str>) -> Result<Value> {
    task::retry(TRIES, rate_limited, async || {
      let mut req = self.drive(self.ctx.http.post(format!("{DRIVE}{path}")));
      if let Some(agent) = agent {
        req = req.header("user-agent", agent);
      }
      self.check(req.json(body).send().await?, path)
    })
    .await
  }

  /// The account page's `data` (nickname, avatar): its envelope differs
  /// from the drive's.
  pub async fn account(&self) -> Result<Value> {
    let resp = self
      .web(self.ctx.http.get(format!("{PAN}/account/info")))
      .query("fr", "pc")
      .query("platform", "pc")
      .send()
      .await?;
    let v = match resp.value() {
      Ok(v) => v,
      Err(_) => return Err(self.status(&resp)),
    };
    match v.at("data") {
      Value::Object(data) if v.bool("success") != Some(false) => Ok(Value::Object(data.clone())),
      _ => Err(self.logged_out(&format!(
        "Quark answered the account page without an account: {}",
        v.str("msg").unwrap_or_else(|| "not logged in".into())
      ))),
    }
  }

  fn drive<'a>(&self, req: Req<'a>) -> Req<'a> {
    let req = req
      .query("pr", "ucpro")
      .query("fr", "pc")
      .query("uc_param_str", "");
    self.web(req)
  }

  fn web<'a>(&self, req: Req<'a>) -> Req<'a> {
    req
      .header("origin", PAN)
      .header("referer", REFERER)
      .header("sec-fetch-site", "same-site")
      .header("sec-fetch-mode", "cors")
      .header("sec-fetch-dest", "empty")
  }

  /// The answer, or the error it carries.
  fn check(&self, resp: Resp, path: &str) -> Result<Value> {
    let Ok(v) = resp.value() else {
      return Err(self.status(&resp));
    };
    let code = v.i64("code").unwrap_or(0);
    let status = v.i64("status").unwrap_or(200);
    if code == 0 && status < 400 && resp.status.is_success() {
      return Ok(v);
    }
    Err(self.refusal(&v, code, status, path))
  }

  /// An answer that is not JSON.
  fn status(&self, resp: &Resp) -> Error {
    if resp.status.as_u16() == 401 {
      return self.logged_out("Quark: not logged in");
    }
    status_error(resp.status, &resp.text())
  }

  fn logged_out(&self, message: &str) -> Error {
    Error::auth(message).with_hint(self.ctx.login_hint())
  }

  /// The atp error for a failed answer: by code, else by its wording.
  fn refusal(&self, v: &Value, code: i64, status: i64, path: &str) -> Error {
    let text = v.str("message").unwrap_or_else(|| "request failed".into());
    let message = format!("Quark {path}: {text} (code {code})");
    let has = |words: &[&str]| {
      let lower = text.to_lowercase();
      words.iter().any(|w| lower.contains(w))
    };
    if code == 31001 || status == 401 || has(&["require login", "未登录"]) {
      return self.logged_out(&message);
    }
    match code {
      23018 => {
        return limit(
          message,
          "Quark limits downloads of this size for this client",
        );
      }
      32003 => return limit(message, "the drive is full; free some space first"),
      23008 => return Error::input(message).with_hint("pick another name"),
      41013 => return Error::not_found(message).with_hint("the target folder is gone"),
      _ => {}
    }
    if has(&["频繁", "too frequent", "rate limit", "稍后"]) {
      Error::new(ErrorCode::RateLimited, message)
    } else if has(&["验证", "captcha", "风控", "risk"]) {
      Error::new(ErrorCode::VerificationRequired, message)
        .with_url(REFERER)
        .with_hint(RISK_HINT)
    } else if has(&["提取码", "passcode", "password"]) {
      Error::input(message).with_hint("pass the share's code with --pwd")
    } else if has(&[
      "不存在",
      "not exist",
      "not found",
      "已删除",
      "过期",
      "expired",
      "失效",
    ]) {
      Error::not_found(message)
    } else if has(&["容量", "capacity", "limit", "超出", "上限"]) || status == 403 {
      Error::new(ErrorCode::PermissionDenied, message)
    } else {
      Error::upstream(message)
    }
  }
}

fn limit(message: String, hint: &str) -> Error {
  Error::new(ErrorCode::PermissionDenied, message).with_hint(hint)
}

fn rate_limited(e: &Error) -> bool {
  e.code == ErrorCode::RateLimited
}

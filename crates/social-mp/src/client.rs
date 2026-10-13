//! What every module works with: the context (with mp.weixin's preferred
//! user agent: article pages show desktop Chrome a captcha) and which of the
//! two logged-in routes are available.
//!
//! - **API**: the official API with the account's AppID and AppSecret, from
//!   `WECHAT_APPID` / `WECHAT_SECRET` (the names md2wechat uses; they win) or
//!   saved by `login --token APPID:SECRET`.
//! - **Backend**: the mp.weixin.qq.com web backend with the cookies of a QR
//!   login (`slave_sid` ...).

use std::cell::RefCell;
use std::collections::BTreeMap;

use social_core::{Ctx, Error, Result};

use crate::web::SESSION_COOKIE;

const SAFARI: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 \
   (KHTML, like Gecko) Version/18.5 Safari/605.1.15";
const APPID_KEY: &str = "appid";
const SECRET_KEY: &str = "secret";

pub const LOGIN_HINT: &str = "run `social mp login` (QR code: the mp.weixin.qq.com backend) or \
   `social mp login --token APPID:SECRET` (official API; WECHAT_APPID / WECHAT_SECRET in the \
   environment work too)";

#[derive(Clone)]
pub struct Credentials {
  pub appid: String,
  pub secret: String,
}

pub struct Client {
  pub ctx: Ctx,
  pub(crate) creds: RefCell<Option<Credentials>>,
  pub(crate) api_token: RefCell<Option<String>>,
  pub(crate) web_token: RefCell<Option<String>>,
  /// Account names already resolved to fakeids in this run.
  pub(crate) accounts: RefCell<BTreeMap<String, String>>,
}

impl Client {
  pub fn new(mut ctx: Ctx) -> Self {
    ctx.http.set_header("user-agent", SAFARI);
    for hint in ["sec-ch-ua", "sec-ch-ua-mobile", "sec-ch-ua-platform"] {
      ctx.http.set_header(hint, "");
    }
    let creds = env_credentials().or_else(|| {
      Some(Credentials {
        appid: ctx.extra(APPID_KEY)?,
        secret: ctx.extra(SECRET_KEY)?,
      })
    });
    Self {
      ctx,
      creds: RefCell::new(creds),
      api_token: RefCell::new(None),
      web_token: RefCell::new(None),
      accounts: RefCell::new(BTreeMap::new()),
    }
  }

  pub fn has_api(&self) -> bool {
    self.creds.borrow().is_some()
  }

  pub fn has_web(&self) -> bool {
    self.ctx.http.has_cookie(SESSION_COOKIE)
  }

  pub fn logged_in(&self) -> bool {
    self.has_api() || self.has_web()
  }

  /// `APPID:SECRET` from `login --token`, kept in the session extras.
  pub fn set_credentials(&self, token: &str) -> Result<()> {
    let (appid, secret) = token
      .split_once(':')
      .map(|(a, s)| (a.trim(), s.trim()))
      .filter(|(a, s)| a.starts_with("wx") && !s.is_empty())
      .ok_or_else(|| {
        Error::input("expected `APPID:SECRET`, the AppID (wx...) and AppSecret of the account")
      })?;
    self.ctx.set_extra(APPID_KEY, appid);
    self.ctx.set_extra(SECRET_KEY, secret);
    *self.creds.borrow_mut() = Some(Credentials {
      appid: appid.into(),
      secret: secret.into(),
    });
    *self.api_token.borrow_mut() = None;
    Ok(())
  }

  /// Fails unless either route is available.
  pub fn require_any(&self) -> Result<()> {
    if self.logged_in() {
      Ok(())
    } else {
      Err(Error::auth("not logged in").with_hint(LOGIN_HINT))
    }
  }
}

fn env_credentials() -> Option<Credentials> {
  let var = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
  Some(Credentials {
    appid: var("WECHAT_APPID")?.trim().to_owned(),
    secret: var("WECHAT_SECRET")?.trim().to_owned(),
  })
}

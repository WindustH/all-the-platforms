//! The transport: the context (with mp.weixin's preferred user agent:
//! article pages show desktop Chrome a captcha) and the two logged-in routes.
//!
//! - [`Official`]: the official API with the account's AppID and AppSecret.
//! - [`Backend`]: the mp.weixin.qq.com web backend with the cookies of a QR
//!   login.

use std::cell::RefCell;
use std::collections::BTreeMap;

use social_core::{Ctx, Error, Result};

use crate::backend::{self, Backend};
use crate::official::{self, Official};

const SAFARI: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 \
   (KHTML, like Gecko) Version/18.5 Safari/605.1.15";

pub const LOGIN_HINT: &str = "run `social mp login` (QR code: the mp.weixin.qq.com backend) or \
   `social mp login --token APPID:SECRET` (official API; WECHAT_APPID / WECHAT_SECRET in the \
   environment work too)";

pub struct Api {
  pub ctx: Ctx,
  official: official::State,
  backend: backend::State,
  /// Account names already resolved to fakeids in this run.
  pub accounts: RefCell<BTreeMap<String, String>>,
}

impl Api {
  pub fn new(mut ctx: Ctx) -> Self {
    ctx.http.set_header("user-agent", SAFARI);
    for hint in ["sec-ch-ua", "sec-ch-ua-mobile", "sec-ch-ua-platform"] {
      ctx.http.set_header(hint, "");
    }
    let official = official::State::new(&ctx);
    Self {
      ctx,
      official,
      backend: backend::State::default(),
      accounts: RefCell::new(BTreeMap::new()),
    }
  }

  /// The official API route.
  pub fn official(&self) -> Official<'_> {
    Official::new(&self.ctx, &self.official)
  }

  /// The mp.weixin.qq.com backend route.
  pub fn backend(&self) -> Backend<'_> {
    Backend::new(&self.ctx, &self.backend)
  }

  pub fn logged_in(&self) -> bool {
    self.official().has() || self.backend().has()
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

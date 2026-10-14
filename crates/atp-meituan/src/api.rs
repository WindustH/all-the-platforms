//! Meituan transport: the 美团联盟 API ([`Cps`]) for deals and orders, and
//! the session's Dianping login for reviews. Requests to Meituan's and
//! Dianping's gateway go through [`atp_mtsec::guard`].

use atp_core::Ctx;

use crate::cps::{self, Cps};

/// The Dianping login cookie the review list wants.
pub const DIANPING_LOGIN: &str = "dper";

pub struct Api {
  pub ctx: Ctx,
  pub cps: Cps,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self {
      ctx,
      cps: Cps::from_env(),
    }
  }

  /// Whether the session holds a Dianping login (imported with the
  /// browser's cookies, which cover dianping.com too).
  pub fn has_dianping(&self) -> bool {
    self.ctx.http.has_cookie(DIANPING_LOGIN)
  }

  /// How to get either route.
  pub fn either_hint(&self) -> String {
    format!(
      "{}; for reviews, log in on www.dianping.com and run `atp mt login --browser`",
      cps::LOGIN_HINT
    )
  }
}

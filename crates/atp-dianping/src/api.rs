//! Dianping transport. Every request goes through [`atp_mtsec::guard`] the
//! way a page of the site sends it, so the gateway's checks, refusals and
//! login walls come back as atp errors. The www site (search, members) and
//! the review list want a login (`dper`); the mobile site serves shop
//! pages to visitors.

use atp_core::credentials;
use atp_core::{Ctx, Error, Result};
use atp_mtsec::guard::{self, Call, DIANPING};

pub use atp_mtsec::shops::{MOBILE, WWW};

/// Cookies of a logged-in session.
pub const LOGIN_COOKIES: &[&str] = &["dper"];
/// Shanghai, Dianping's own default.
const DEFAULT_CITY: &str = "1";

/// Channels (`search --filter`, `feed --type`) with Dianping's ids.
const CHANNELS: &[(&str, &str)] = &[
  ("food", "10"),
  ("shopping", "20"),
  ("shows", "25"),
  ("fun", "30"),
  ("trips", "35"),
  ("sports", "45"),
  ("beauty", "50"),
  ("wedding", "55"),
  ("car", "65"),
  ("family", "70"),
  ("learning", "75"),
  ("services", "80"),
  ("health", "85"),
  ("home", "90"),
  ("pets", "95"),
];

pub const CHANNEL_NAMES: &[&str] = &[
  "food", "shopping", "shows", "fun", "trips", "sports", "beauty", "wedding", "car", "family",
  "learning", "services", "health", "home", "pets",
];

/// The id of a channel name.
pub fn channel(name: Option<&str>) -> Result<Option<&'static str>> {
  let Some(name) = name else { return Ok(None) };
  CHANNELS
    .iter()
    .find(|(n, _)| *n == name)
    .map(|(_, id)| Some(*id))
    .ok_or_else(|| Error::input(format!("unknown channel {name}")))
}

pub struct Api {
  pub ctx: Ctx,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self { ctx }
  }

  pub fn require_login(&self) -> Result<()> {
    self.ctx.require_login(LOGIN_COOKIES)
  }

  /// The city searches run in: `DIANPING_CITY`, else the session's city
  /// (cookie `cy`), else Shanghai.
  pub fn city(&self) -> String {
    credentials::env("DIANPING_CITY")
      .or_else(|| self.ctx.http.cookie("cy"))
      .filter(|c| c.chars().all(|ch| ch.is_ascii_digit()))
      .unwrap_or_else(|| DEFAULT_CITY.to_owned())
  }

  pub async fn html(&self, call: Call) -> Result<String> {
    guard::html(&self.ctx, &DIANPING, call).await
  }
}

/// Images as `//host/…` or `http://…`, made `https://`.
pub fn absolute(link: &str) -> String {
  let link = link.trim();
  if let Some(rest) = link.strip_prefix("//") {
    format!("https://{rest}")
  } else {
    link.replacen("http://", "https://", 1)
  }
}

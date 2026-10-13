//! How requests present themselves as the Discord web client: the
//! `X-Super-Properties` header and the current client build number.

use std::time::Duration;

use atp_core::http::USER_AGENT;
use atp_core::{Value, json};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use super::{Api, WEB};

/// Client build of the web app when discord.com cannot be asked.
const BUILD_FALLBACK: u64 = 634_304;
const BUILD_TTL: Duration = Duration::from_secs(24 * 3600);

impl Api {
  /// `X-Super-Properties`: the web client's self-description, base64 JSON.
  pub async fn super_properties(&self) -> String {
    if let Some(p) = self.props.borrow().clone() {
      return p;
    }
    let props = STANDARD.encode(self.client_properties().await.to_string());
    *self.props.borrow_mut() = Some(props.clone());
    props
  }

  /// The client description sent in `X-Super-Properties` and the gateway identify.
  pub async fn client_properties(&self) -> Value {
    json!({
      "os": "Mac OS X",
      "browser": "Chrome",
      "device": "",
      "system_locale": "en-US",
      "has_client_mods": false,
      "browser_user_agent": USER_AGENT,
      "browser_version": "149.0.0.0",
      "os_version": "10.15.7",
      "referrer": "",
      "referring_domain": "",
      "referrer_current": "",
      "referring_domain_current": "",
      "release_channel": "stable",
      "client_build_number": self.build_number().await,
      "client_event_source": null,
    })
  }

  /// The web app's current build number, read from discord.com once a day.
  async fn build_number(&self) -> u64 {
    if let Some(n) = self.ctx.store.cache_get::<u64>("build", BUILD_TTL) {
      return n;
    }
    let page = self
      .ctx
      .http
      .get(format!("{WEB}/login"))
      .no_cookies()
      .header("accept", "text/html")
      .send()
      .await;
    let found = page.ok().and_then(|r| {
      let text = r.text();
      let at = text.find("\"BUILD_NUMBER\":\"")? + 16;
      text[at..].split('"').next()?.parse::<u64>().ok()
    });
    match found {
      Some(n) => {
        self.ctx.store.cache_put("build", &n);
        n
      }
      None => {
        tracing::debug!("no build number on discord.com; using {BUILD_FALLBACK}");
        BUILD_FALLBACK
      }
    }
  }
}

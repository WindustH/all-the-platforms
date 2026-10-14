//! `anti_content`, the token PDD's H5 pages put on their `/proxy/api`
//! calls, ported from chunk `3636_<hash>.js` (`RiskControlCrawler`)
//! without running it: the chunk is a bytecode VM whose programs were
//! decrypted and read statically. The token is a deflated record of what
//! the page knows about the browser (link, screen, user agent, cookies,
//! fingerprint id, load and server times, a call counter), not a keyed
//! signature; the server checks that it is well formed and plausible.
//!
//! One [`Signer`] stands for one page load: it starts on the first token,
//! takes the server time once and counts its tokens. The fingerprint id
//! and the tails of earlier tokens persist like `localStorage` does.

mod device;
mod encode;
mod message;

use std::cell::RefCell;
use std::time::Duration;

use atp_core::Ctx;

use device::Device;
pub use device::USER_AGENT;
use message::Inputs;

const DEVICE_KEY: &str = "device";
const DEVICE_TTL: Duration = Duration::from_secs(90 * 24 * 3600);
/// The fingerprint cookie the page sets next to its `localStorage` copy.
const NANO_FP: &str = "_nano_fp";

/// One page load of the generator.
struct Page {
  href: String,
  loaded: i64,
  server_time: i64,
  synced: i64,
  count: u64,
}

#[derive(Default)]
pub struct Signer {
  page: RefCell<Option<Page>>,
  device: RefCell<Option<Device>>,
}

impl Signer {
  /// Whether a page is loaded (and its server time known).
  pub fn started(&self) -> bool {
    self.page.borrow().is_some()
  }

  /// Load the page `href` with the server's time (ms).
  pub fn start(&self, href: &str, server_time: i64) {
    let now = now_ms();
    *self.page.borrow_mut() = Some(Page {
      href: href.to_owned(),
      loaded: now - 300,
      server_time,
      synced: now,
      count: 0,
    });
  }

  /// A fresh token for a call made from the loaded page.
  pub fn token(&self, ctx: &Ctx, referrer: &str) -> String {
    let mut device = self.device(ctx);
    let mut page = self.page.borrow_mut();
    let page = page.get_or_insert_with(|| Page {
      href: format!("{}/", crate::api::WEB),
      loaded: now_ms(),
      server_time: now_ms(),
      synced: now_ms(),
      count: 0,
    });
    page.count += 1;
    let cookie = |name: &str| ctx.http.cookie(name).map(|v| unescape(&v));
    let inputs = Inputs {
      href: &page.href,
      referrer,
      user_id: cookie("pdd_user_id"),
      api_uid: cookie("api_uid"),
      vds: cookie("pdd_vds"),
      count: page.count,
      loaded: page.loaded,
      server_time: page.server_time,
      since_sync: (now_ms() - page.synced).max(0) as u64,
      nano_fp: &device.nano_fp,
      tails: &device.tails,
    };
    let token = message::token(&message::body(&inputs));
    device.remember(&token);
    ctx.store.cache_put(DEVICE_KEY, &device);
    *self.device.borrow_mut() = Some(device);
    token
  }

  /// The browser state: the session's `_nano_fp` cookie when it has one,
  /// else the cached device, else a new one; the cookie is kept in step.
  fn device(&self, ctx: &Ctx) -> Device {
    let mut device = self
      .device
      .borrow()
      .clone()
      .or_else(|| ctx.store.cache_get::<Device>(DEVICE_KEY, DEVICE_TTL))
      .unwrap_or_else(|| Device::new(now_ms()));
    if let Some(fp) = ctx.http.cookie(NANO_FP).filter(|v| v.len() > 20) {
      device.nano_fp = fp;
    } else {
      ctx.http.set_cookie(NANO_FP, &device.nano_fp);
    }
    device
  }
}

fn now_ms() -> i64 {
  jiff::Timestamp::now().as_millisecond()
}

/// `%XX` sequences decoded, as the page's cookie reader does.
fn unescape(text: &str) -> String {
  let bytes = text.as_bytes();
  let mut out = Vec::with_capacity(bytes.len());
  let mut i = 0;
  while i < bytes.len() {
    let hex = bytes
      .get(i + 1..i + 3)
      .and_then(|h| std::str::from_utf8(h).ok())
      .and_then(|h| u8::from_str_radix(h, 16).ok());
    match (bytes[i], hex) {
      (b'%', Some(byte)) => {
        out.push(byte);
        i += 3;
      }
      (byte, _) => {
        out.push(byte);
        i += 1;
      }
    }
  }
  String::from_utf8_lossy(&out).into_owned()
}

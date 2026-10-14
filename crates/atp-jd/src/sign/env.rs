//! The browser environment the library collects: once in full when it asks
//! `request_algo` for a token, and a shorter form inside every h5st. Values
//! describe the Chrome atp's HTTP client imitates (its User-Agent decides
//! the system) on the page the call is made from; the canvas hash is drawn
//! once per installation, like a device's.

use std::sync::LazyLock;

use atp_core::{Value, json};
use regex::Regex;

use super::token::nonce;

/// The library build the environment names.
pub const FILE_VERSION: &str = "h5_file_v5.3.5";
/// Collector version (`extend.bu1`).
const COLLECTOR: &str = "0.1.6";
/// A label the library sends unchanged.
const LABEL: &str = "VRAqLjAEAm";
const WINDOW: &str = "function Window() { [native code] }$function toString() { [native code] }";
const SECURITY_JS: &str =
  "https://storage.360buyimg.com/webcontainer/js_security_v3_0.1.6.js?v=2024-06-20-17:5";
/// Chrome's language list matching atp's `accept-language`.
const LANGUAGES: &str = "zh-CN,zh,en";
const CORES: u32 = 10;
/// `getTimezoneOffset() / 60` in China.
const TIMEZONE: i32 = -8;

static SYSTEM: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"Mozilla/5\.0 \((.*?)\)").expect("valid"));

/// What the environment needs to know about the client and the page.
pub struct Context<'a> {
  pub user_agent: &'a str,
  /// Canvas fingerprint (an MD5 hex).
  pub canvas: &'a str,
  /// `pwdt_id`, `pin` and `pt_pin` of a logged-in session.
  pub pins: [Option<String>; 3],
  /// The page's own cookies, sent when there is no pin.
  pub cookies: String,
  pub page: &'a str,
  pub origin: &'a str,
}

impl Context<'_> {
  fn system(&self) -> String {
    SYSTEM
      .captures(self.user_agent)
      .map(|m| m[1].to_owned())
      .unwrap_or_default()
  }

  fn platform(&self) -> &'static str {
    let ua = self.user_agent;
    if ua.contains("Macintosh") {
      "MacIntel"
    } else if ua.contains("Windows") {
      "Win32"
    } else {
      "Linux x86_64"
    }
  }

  fn pins(&self) -> Value {
    let mut pins = serde_json::Map::new();
    for (key, value) in ["p1", "p2", "p3"].iter().zip(&self.pins) {
      if let Some(value) = value {
        pins.insert((*key).into(), json!(value));
      }
    }
    Value::Object(pins)
  }

  /// `[w, h, outerWidth, outerHeight, devicePixelRatio]`.
  fn screen(&self) -> [u32; 5] {
    if self.user_agent.contains("Macintosh") {
      [1512, 982, 1512, 944, 2]
    } else {
      [1920, 1080, 1920, 1032, 1]
    }
  }

  fn extend(&self, sites: u32) -> Value {
    json!({
      "wd": 0, "l": 0, "ls": 5, "wk": 0, "bu1": COLLECTOR, "bu3": 43, "bu4": 0, "bu5": 0,
      "bu6": sites, "bu7": 0, "bu8": 0, "random": nonce(10), "bu12": TIMEZONE, "bu10": 14,
      "bu11": 4,
    })
  }

  /// Sent to `request_algo` as `expandParams`.
  pub fn full(&self, app_id: &str, fp: &str) -> Value {
    let [w, h, ow, oh, ratio] = self.screen();
    let logged_in = self.pins.iter().any(Option::is_some);
    json!({
      "wc": 0, "wd": 0, "l": "zh-CN", "ls": LANGUAGES, "ml": 2, "pl": 5,
      "av": self.user_agent.trim_start_matches("Mozilla/"),
      "ua": self.user_agent,
      "sua": self.system(),
      "pp": self.pins(),
      "extend": self.extend(21),
      "pp1": if logged_in { String::new() } else { self.cookies.clone() },
      "bu1": format!("Error: test err\n    at HTMLDocument.querySelector ({SECURITY_JS}:9219)"),
      "w": w, "h": h, "ow": ow, "oh": oh,
      "url": self.page, "og": self.origin,
      "pf": self.platform(), "pr": ratio, "re": "",
      "random": nonce(11), "referer": "", "v": FILE_VERSION,
      "bu2": format!("    at {SECURITY_JS}:71875"),
      "bu3": WINDOW, "bu4": "0",
      "canvas": self.canvas, "canvas1": self.canvas,
      "webglFp": "", "ccn": CORES, "bu14": LABEL,
      "ai": app_id, "fp": fp, "wk": -1,
    })
  }

  /// Part 8 of every h5st.
  pub fn short(&self, fp: &str, now_ms: i64) -> Value {
    json!({
      "sua": self.system(),
      "pp": self.pins(),
      "extend": self.extend(22),
      "pf": self.platform(),
      "random": nonce(11),
      "v": FILE_VERSION,
      "bu4": "0",
      "canvas": self.canvas,
      "webglFp": "",
      "ccn": CORES,
      "bu14": LABEL,
      "t": now_ms,
      "fp": fp,
    })
  }
}

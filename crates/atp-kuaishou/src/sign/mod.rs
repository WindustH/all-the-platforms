//! Request signatures of the Kuaishou web, ported from catbus (MIT), itself
//! a port of KuaiShou-Spider's pure signers:
//!
//! - `__NS_hxfalcon` ([`falcon`], "sig4") on the www paths the web SDK
//!   lists, on the live site and on the login site, sent with `caver=2`;
//! - `__NS_sig3` ([`sig3`]) on the creator center;
//! - the webweapon `kwfv1` cookie and `kww` header ([`weapon`]).
//!
//! The signers keep a start time and a call counter, like the page session
//! of a browser: one [`crate::api::Api`] holds one of each.

pub mod falcon;
pub mod sig3;
pub mod weapon;

pub const CAVER: &str = "2";

/// www paths the web SDK signs.
const WWW_SIGNED: &[&str] = &[
  "/rest/v/profile/get",
  "/rest/v/profile/user/v2",
  "/rest/v/search/user",
  "/rest/v/search/feed",
  "/rest/v/profile/feed",
  "/rest/v/feed/hot",
  "/rest/v/feed/liked",
  "/rest/v/collect/list",
  "/rest/v/profile/private/list",
];

/// Live paths the web signs: the path it calls → the path behind the
/// gateway, which the signature covers.
const LIVE_SIGNED: &[(&str, &str)] = &[
  ("/live_api/comment/list", "/rest/k/photo/comment/list"),
  ("/live_api/profile/public", "/rest/k/feed/profile"),
  (
    "/live_api/liveroom/websocketinfo",
    "/rest/k/live/websocket/info",
  ),
  ("/live_api/profile/feedbyid", "/rest/k/photo"),
  ("/live_api/playback/list", "/rest/k/playback/product/list"),
  ("/live_api/baseuser/userinfo/byid", "/rest/k/user/info"),
  ("/live_api/baseuser/userLogin", "/rest/k/user/info"),
  (
    "/live_api/liveroom/status",
    "/rest/wd/live/liveStream/status",
  ),
];

/// Login-site paths signed over their form fields.
const LOGIN_SIGNED: &[&str] = &[
  "/rest/c/infra/ks/qr/start",
  "/rest/c/infra/ks/qr/scanResult",
  "/rest/c/infra/ks/qr/acceptResult",
  "/pass/kuaishou/login/qr/callback",
];

/// Creator-center paths signed with sig3.
const SIG3_MARKERS: &[&str] = &["/rest/cp", "rest/v2/creator", "/rest/kd"];

pub fn www_signed(path: &str) -> bool {
  WWW_SIGNED.contains(&path)
}

/// The gateway path a live call is signed over, when it is signed.
pub fn live_signed(path: &str) -> Option<&'static str> {
  LIVE_SIGNED
    .iter()
    .find(|(web, _)| *web == path)
    .map(|(_, real)| *real)
}

pub fn login_signed(path: &str) -> bool {
  LOGIN_SIGNED.contains(&path)
}

pub fn sig3_signed(path: &str) -> bool {
  SIG3_MARKERS.iter().any(|m| path.contains(m))
}

/// What sig4 covers: the path, the sorted `k=v` pairs of the query (with
/// `caver`) and the form, then the JSON body when there is one.
pub fn falcon_input(path: &str, pairs: &[(String, String)], body: Option<&str>) -> String {
  let mut items: Vec<String> = std::iter::once(("caver", CAVER))
    .chain(pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
    .filter(|(k, _)| !k.contains("__NS"))
    .map(|(k, v)| format!("{k}={v}"))
    .collect();
  items.sort();
  let mut out = path.to_owned();
  out.extend(items);
  out.push_str(body.unwrap_or_default());
  out
}

/// A query component as axios writes it: `encodeURIComponent`, then
/// `: $ , [ ]` back as they were and spaces as `+`.
pub fn component(s: &str) -> String {
  escape(s, b"-_.!~*'():$,[]").replace("%20", "+")
}

/// JavaScript's `encodeURI`.
pub fn encode_uri(s: &str) -> String {
  escape(s, b"-_.!~*'();,/?:@&=+$#")
}

fn escape(s: &str, keep: &[u8]) -> String {
  let mut out = String::with_capacity(s.len());
  for b in s.bytes() {
    if b.is_ascii_alphanumeric() || keep.contains(&b) {
      out.push(b as char);
    } else {
      out.push_str(&format!("%{b:02X}"));
    }
  }
  out
}

/// `a=1&b=2` with axios' escaping.
pub fn query_string(pairs: &[(String, String)]) -> String {
  pairs
    .iter()
    .map(|(k, v)| format!("{}={}", component(k), component(v)))
    .collect::<Vec<_>>()
    .join("&")
}

/// A form body as the login page sends it: every value fully escaped.
pub fn form_string(pairs: &[(String, String)]) -> String {
  pairs
    .iter()
    .map(|(k, v)| format!("{}={}", escape(k, b"-_.~"), escape(v, b"-_.~")))
    .collect::<Vec<_>>()
    .join("&")
}

pub fn hex(bytes: &[u8]) -> String {
  bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The SDK's one-byte checksum: the byte sum, negated once it passes 255.
pub fn checksum(bytes: &[u8]) -> u8 {
  let total: u32 = bytes.iter().map(|&b| u32::from(b)).sum();
  if total > 255 {
    (total.wrapping_neg() & 0xff) as u8
  } else {
    total as u8
  }
}

/// Unix time in milliseconds.
pub fn now_ms() -> u64 {
  jiff::Timestamp::now().as_millisecond() as u64
}

//! How Meituan's gateway says no, as atp errors.
//!
//! - A redirect to the Yoda check (`verify.meituan.com`, `action=
//!   spiderindefence`) or to MTSI (`optimus-mtsi.meituan.com`):
//!   `verification_required` with that page.
//! - JSON naming a check (code or `yodaCode` 406, with a `requestCode` or a
//!   `verifyPageUrl`): the same, with the check page the gateway's own
//!   redirects lead to.
//! - HTTP 403 from the gateway (`X-Forbid-Reason`, openresty's page) or
//!   418: `verification_required` with the page the call came from. Opened
//!   in the person's browser it shows the check if there is one; passing
//!   it there lets atp's retry through, with that browser's cookies.
//! - A redirect to a login page, or a login error (401,
//!   `sys_err_auth_fail`): `not_authenticated`.
//! - Account risk (101144 `user_has_risk`): `permission_denied`; throttling
//!   (200100, `频繁`, HTTP 429): `rate_limited`.

use atp_core::http::Resp;
use atp_core::text::encode_uri_component;
use atp_core::{Ctx, Error, ErrorCode, Value, ValueExt};

use crate::guard::Site;

const CHECK_HINT: &str = "pass the check in the browser atp opens (or open the page \
  yourself), then try again; keep requests sparse";
const FORBIDDEN_HINT: &str = "open the page in your browser and pass any check there, then \
  try again (atp offers this on a terminal); if it keeps failing, this route wants the page's \
  own request signature (mtgsig), which atp does not make";
const RISK_HINT: &str = "atp stops here rather than retry: use the app for a while or wait \
  before trying again";
const CHECK_HOSTS: [&str; 2] = ["verify.meituan.com", "optimus-mtsi.meituan.com"];
const LOGIN_HOSTS: [&str; 3] = [
  "account.dianping.com",
  "passport.meituan.com",
  "passport.dianping.com",
];

/// A refusal in how the gateway answered: where it redirected to, or its
/// status.
pub fn of_response(ctx: &Ctx, site: &Site, resp: &Resp, from: &str) -> Option<Error> {
  let host = host(&resp.url);
  if CHECK_HOSTS.contains(&host.as_str()) || resp.url.contains("spiderindefence") {
    return Some(check(site, &resp.url));
  }
  if LOGIN_HOSTS.contains(&host.as_str()) {
    return Some(login(ctx, site));
  }
  match resp.status.as_u16() {
    403 => Some(forbidden(site, resp, from)),
    418 => Some(
      resp
        .value()
        .ok()
        .and_then(|v| of_body(ctx, site, &v, from))
        .unwrap_or_else(|| check(site, from)),
    ),
    429 => Some(throttled(site, "HTTP 429")),
    460 => Some(Error::new(
      ErrorCode::PermissionDenied,
      format!("{} refuses this route to web clients (HTTP 460)", site.name),
    )),
    _ => None,
  }
}

/// A refusal written in a JSON answer.
pub fn of_body(ctx: &Ctx, site: &Site, v: &Value, from: &str) -> Option<Error> {
  if let Some(page) = check_url(v, from) {
    return Some(check(site, &page));
  }
  let code = code(v);
  let message = v
    .first_str(&["message", "msg", "error.message", "data.message"])
    .unwrap_or_default();
  if code == Some(401) || v.str("error.type").as_deref() == Some("sys_err_auth_fail") {
    return Some(login(ctx, site));
  }
  if code == Some(101144) || message.contains("user_has_risk") {
    return Some(
      Error::new(
        ErrorCode::PermissionDenied,
        format!(
          "{} flags this account's activity: {message} (101144)",
          site.name
        ),
      )
      .with_hint(RISK_HINT),
    );
  }
  if code == Some(200100) || message.contains("频繁") {
    return Some(throttled(site, &message));
  }
  None
}

/// The check page a JSON answer names, or builds from its `requestCode`.
fn check_url(v: &Value, from: &str) -> Option<String> {
  let named = v.first_str(&[
    "customData.verifyPageUrl",
    "customData.verifyUrl",
    "data.customData.verifyPageUrl",
    "data.verifyPageUrl",
    "data.verifyUrl",
    "verifyPageUrl",
  ]);
  if let Some(page) = named {
    return Some(absolute(&page));
  }
  let yoda = code(v) == Some(406) || v.i64("data.yodaCode") == Some(406);
  if !yoda {
    return None;
  }
  let request = v.first_str(&[
    "customData.requestCode",
    "data.customData.requestCode",
    "data.requestCode",
    "requestCode",
  ]);
  Some(match request {
    Some(code) => check_page(&code, from),
    None => from.to_owned(),
  })
}

/// The Yoda page for a request code, returning to `from` once passed, as
/// the gateway's redirects name it.
pub fn check_page(request_code: &str, from: &str) -> String {
  let back = format!(
    "https://optimus-mtsi.meituan.com/optimus/verifyResult?originUrl={}",
    encode_uri_component(from)
  );
  format!(
    "https://verify.meituan.com/v2/app/general_page?requestCode={}&adaptor=auto\
     &action=spiderindefence&succCallbackUrl={}",
    encode_uri_component(request_code),
    encode_uri_component(&back)
  )
}

fn code(v: &Value) -> Option<i64> {
  v.i64("code")
    .or_else(|| v.i64("error.code"))
    .or_else(|| v.i64("data.code"))
}

fn check(site: &Site, page: &str) -> Error {
  Error::new(
    ErrorCode::VerificationRequired,
    format!(
      "{} wants a human check (Yoda) before this request",
      site.name
    ),
  )
  .with_url(page)
  .with_hint(CHECK_HINT)
}

fn forbidden(site: &Site, resp: &Resp, from: &str) -> Error {
  let code = resp
    .header("x-forbid-code")
    .map(|c| format!(", {c}"))
    .unwrap_or_default();
  Error::new(
    ErrorCode::VerificationRequired,
    format!(
      "{}'s gateway refused the request (HTTP 403{code})",
      site.name
    ),
  )
  .with_url(from)
  .with_hint(FORBIDDEN_HINT)
}

fn login(ctx: &Ctx, site: &Site) -> Error {
  Error::auth(format!("{} wants a login for this", site.name)).with_hint(ctx.login_hint())
}

fn throttled(site: &Site, why: &str) -> Error {
  Error::new(
    ErrorCode::RateLimited,
    format!("{} is throttling this client ({why})", site.name),
  )
  .with_hint("wait a few minutes before trying again")
}

fn host(link: &str) -> String {
  url::Url::parse(link)
    .ok()
    .and_then(|u| u.host_str().map(str::to_owned))
    .unwrap_or_default()
}

fn absolute(link: &str) -> String {
  match link.strip_prefix("//") {
    Some(rest) => format!("https://{rest}"),
    None => link.to_owned(),
  }
}

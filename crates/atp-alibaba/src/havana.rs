//! Alibaba's Havana passport, as both sites' login pages run its QR login:
//! `generate.do` hands out a code (`codeContent`) with the `t` and `ck`
//! that `query.do` is then polled with, until the scan is confirmed or the
//! code expires. A confirmed login may first want the person's identity
//! checked (a face or SMS check) on a page of its own: that is
//! `verification_required` with that page.

use atp_core::{Error, ErrorCode, QrStatus, Result, Value, ValueExt};

/// A code to scan and what its polls carry.
pub struct Code {
  /// What the QR code encodes.
  pub content: String,
  pub t: String,
  pub ck: String,
}

/// The code in a `generate.do` answer.
pub fn code(answer: &Value, site: &str) -> Result<Code> {
  let data = answer.at("content.data");
  let (Some(content), Some(t)) = (data.str("codeContent"), data.str("t")) else {
    return Err(Error::upstream(format!(
      "the {site} passport issued no QR code"
    )));
  };
  Ok(Code {
    content,
    t,
    ck: data.str("ck").unwrap_or_default(),
  })
}

/// How the code stands after a `query.do`, with the answer's data (which
/// finishes a confirmed login). A refused code is an error.
pub fn status(answer: &Value) -> Result<(QrStatus, &Value)> {
  let data = answer.at("content.data");
  let status = match data.str("qrCodeStatus").as_deref() {
    Some("SCANED" | "SCANNED") => QrStatus::Scanned,
    Some("CONFIRMED") => QrStatus::Confirmed,
    Some("EXPIRED" | "CANCELED" | "CANCELLED") => QrStatus::Expired,
    Some("ERROR") => {
      let message = data.str("titleMsg").filter(|m| !m.is_empty());
      return Err(Error::upstream(
        message.unwrap_or_else(|| "the passport refused the QR login".into()),
      ));
    }
    _ => QrStatus::Waiting,
  };
  Ok((status, data))
}

/// The page a confirmed login goes on to: where it ends, or the identity
/// check it wants first.
pub fn redirect(data: &Value) -> Option<String> {
  data.first_str(&["iframeRedirectUrl", "redirectUrl", "parentRedirectUrl"])
}

/// Whether a page is an identity check (face, SMS) rather than the end of
/// the login.
pub fn is_check(url: &str) -> bool {
  [
    "havanaappiv",
    "/iv/",
    "identity_verify",
    "login_unusual",
    "member/verify",
  ]
  .iter()
  .any(|w| url.contains(w))
}

/// A login the passport wants confirmed first. `id` is the platform's
/// command name, `home` its website.
pub fn check_wanted(site: &str, id: &str, home: &str, url: Option<String>) -> Error {
  let error = Error::new(
    ErrorCode::VerificationRequired,
    format!("{site} wants to confirm this login (a face or SMS check)"),
  );
  match url {
    Some(url) => error
      .with_hint(format!(
        "finish the check on that page in a browser, then run `atp {id} login --browser`"
      ))
      .with_url(url),
    None => error.with_hint(format!(
      "log in on {home} in a browser, then run `atp {id} login --browser`"
    )),
  }
}

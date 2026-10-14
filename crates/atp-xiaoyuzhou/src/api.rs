//! Xiaoyuzhou transport: the public pages of www.xiaoyuzhoufm.com. The
//! site renders them on its server with Next.js, and each page carries
//! what it shows in `__NEXT_DATA__` (`props.pageProps`): a show with its
//! newest episodes, an episode with its show notes and hot comments, a
//! user with the shows they host. No account and no app API: the
//! logged-in API of Xiaoyuzhou's app (`api.xiaoyuzhoufm.com`) is never
//! called.
//!
//! A page that comes back without its data and looks like a human check
//! (a firewall or captcha page) is `verification_required` with the page
//! to open.

use atp_core::http::status_error;
use atp_core::json::next_data;
use atp_core::text::{one_line, truncate};
use atp_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt};

pub const BASE: &str = "https://www.xiaoyuzhoufm.com";
/// Words of the pages Xiaoyuzhou's firewall answers with instead of a page.
const CHECK_WORDS: &[&str] = &[
  "captcha",
  "验证",
  "acw_sc",
  "_waf",
  "aliyun_waf",
  "访问异常",
];

pub struct Api {
  pub ctx: Ctx,
}

impl Api {
  pub fn new(ctx: Ctx) -> Self {
    Self { ctx }
  }

  /// The `pageProps` of the page at `path` (`/podcast/<id>`).
  pub async fn page(&self, path: &str) -> Result<Value> {
    let url = format!("{BASE}{path}");
    let resp = self
      .ctx
      .http
      .get(&url)
      .header("accept", "text/html,application/xhtml+xml")
      .send()
      .await?;
    let status = resp.status.as_u16();
    if status == 404 {
      return Err(missing(path));
    }
    let html = resp.text();
    let Some(data) = next_data(&html) else {
      return Err(without_data(resp.status, &html, url));
    };
    let props = data.at("props.pageProps").clone();
    if props.is_null() || props.i64("statusCode").is_some_and(|code| code == 404) {
      return Err(missing(path));
    }
    Ok(props)
  }
}

fn missing(path: &str) -> Error {
  Error::not_found(format!("Xiaoyuzhou has no page {BASE}{path}"))
}

/// A page without `__NEXT_DATA__`: a human check, an HTTP error, or a
/// page ATP does not know.
fn without_data(status: atp_core::http::StatusCode, html: &str, url: String) -> Error {
  let check = CHECK_WORDS.iter().any(|word| html.contains(word));
  if check || matches!(status.as_u16(), 403 | 405 | 429) {
    return Error::new(
      ErrorCode::VerificationRequired,
      "Xiaoyuzhou answered with a check instead of the page",
    )
    .with_url(url)
    .with_hint(
      "open the page in a browser and pass the check, then retry; slow down with `--interval`",
    );
  }
  if !status.is_success() {
    return status_error(status, html);
  }
  Error::upstream(format!(
    "Xiaoyuzhou's page {url} carried no data: {}",
    truncate(&one_line(html), 120)
  ))
}

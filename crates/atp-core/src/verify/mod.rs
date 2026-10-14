//! Human checks done by hand: when a platform answers
//! `verification_required` on a terminal, the check opens in a browser that
//! carries the session's cookies; once the person has passed it, the cookies
//! the site set come back into the session and the command runs again.
//!
//! A Chromium-family browser (Chrome, Chromium, Edge, Brave) is driven over
//! DevTools on a throwaway profile, through the same proxy as atp so the
//! site sees the same address. Without one, the default browser opens the
//! page and the command is retried with the session as it was.

mod cdp;
mod chrome;

use std::io::IsTerminal;
use std::time::Duration;

use self::cdp::Cdp;
use self::chrome::Browser;
use crate::error::{Error, Result};
use crate::http::{Cookies, proxy_url};
use crate::output;
use crate::platform::PlatformInfo;

/// How often the browser's cookies are copied while the person works.
const POLL: Duration = Duration::from_secs(2);

/// Whether a check may be offered: someone is at the terminal and
/// `--no-verify` / `ATP_NO_VERIFY` is not set.
pub fn wanted(disabled: bool) -> bool {
  !disabled && std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

/// Let the person pass the check behind `error`. `Some(cookies)` means
/// "retry with these cookies merged in" (possibly none); `None` means they
/// gave up or no browser could be opened.
pub async fn complete(
  info: &PlatformInfo,
  error: &Error,
  cookies: &Cookies,
  proxy: Option<&str>,
) -> Option<Cookies> {
  let url = error.link().unwrap_or_else(|| info.home.to_owned());
  eprintln!("{} wants a human check: {}", info.name, error.message);
  let Some(exe) = chrome::find() else {
    return fallback(&url).await;
  };
  match drive(&exe, info, &url, cookies, proxy).await {
    Ok(found) => found,
    Err(e) => {
      output::note(&format!("could not drive {}: {e}", exe.display()));
      fallback(&url).await
    }
  }
}

async fn drive(
  exe: &std::path::Path,
  info: &PlatformInfo,
  url: &str,
  cookies: &Cookies,
  proxy: Option<&str>,
) -> Result<Option<Cookies>> {
  let proxy = proxy_url(proxy).and_then(|p| browser_proxy(&p));
  let mut browser = Browser::launch(exe, proxy.as_deref())?;
  let mut cdp = Cdp::connect(&browser.endpoint().await?).await?;
  let domains = info.cookie_domains;
  cdp.set_cookies(cookies, domains).await?;
  cdp.open(url).await?;
  eprintln!("Opened in {}: {url}", name(exe));
  let mut latest = cdp.cookies(domains).await?;
  let done = wait_for_enter(async || {
    if !browser.exited()
      && let Ok(now) = cdp.cookies(domains).await
    {
      latest = now;
    }
  })
  .await;
  if !browser.exited() {
    if let Ok(now) = cdp.cookies(domains).await {
      latest = now;
    }
    cdp.close().await;
    browser.wait().await;
  }
  Ok(done.then_some(latest))
}

/// Open the page in the default browser and retry with the session as is.
async fn fallback(url: &str) -> Option<Cookies> {
  let opened = chrome::open_default(url);
  eprintln!(
    "No Chrome, Chromium, Edge or Brave found (ATP_BROWSER can name one); {}: {url}",
    if opened { "opened in the default browser" } else { "open it yourself" }
  );
  eprintln!("Cookies set there do not reach atp; if the retry still fails, log in again.");
  wait_for_enter(async || {}).await.then(Cookies::new)
}

/// Prompt, then run `tick` every [`POLL`] until a line is entered:
/// `true` for a plain Enter (retry), `false` for `q` or a closed stdin.
async fn wait_for_enter(mut tick: impl AsyncFnMut()) -> bool {
  eprintln!("Pass the check there, then press Enter here to retry (q and Enter gives up).");
  let mut line = tokio::task::spawn_blocking(|| {
    let mut text = String::new();
    std::io::stdin().read_line(&mut text).map(|n| (n, text))
  });
  loop {
    tokio::select! {
      read = &mut line => {
        return matches!(read, Ok(Ok((n, text))) if n > 0 && !text.trim().eq_ignore_ascii_case("q"));
      }
      () = tokio::time::sleep(POLL) => tick().await,
    }
  }
}

/// `proxy` as Chrome's `--proxy-server` takes it: no credentials, `socks5`
/// for `socks5h`.
fn browser_proxy(proxy: &str) -> Option<String> {
  let mut parsed = url::Url::parse(proxy).ok()?;
  if !parsed.username().is_empty() {
    output::note("the browser cannot use the proxy's password; it may ask for it");
    let _ = parsed.set_username("");
    let _ = parsed.set_password(None);
  }
  let scheme = match parsed.scheme() {
    "socks5h" => "socks5",
    other => other,
  };
  Some(format!(
    "{scheme}://{}:{}",
    parsed.host_str()?,
    parsed.port_or_known_default()?
  ))
}

fn name(exe: &std::path::Path) -> String {
  exe
    .file_stem()
    .map(|s| s.to_string_lossy().into_owned())
    .unwrap_or_else(|| "the browser".into())
}

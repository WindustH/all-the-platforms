//! Login, logout and status, shared by every platform.

use std::process::ExitCode;
use std::time::{Duration, Instant};

use jiff::Timestamp;

use crate::browser;
use crate::cli::LoginArgs;
use crate::ctx::Ctx;
use crate::error::{Error, ErrorCode, Result};
use crate::model::{Action, AuthStatus, Data};
use crate::output::note;
use crate::platform::{Cap, Platform, PlatformInfo, QrStatus};
use crate::qr;
use crate::store::{Session, Store};

const QR_TIMEOUT: Duration = Duration::from_secs(180);
const QR_POLL: Duration = Duration::from_secs(2);

pub struct Loaded {
  pub session: Session,
  /// Cookies came from `ATP_<ID>_COOKIE`; never written back to disk.
  pub from_env: bool,
}

fn env_var(info: &PlatformInfo) -> String {
  format!("ATP_{}_COOKIE", info.id.to_ascii_uppercase())
}

/// The session to use: `ATP_<ID>_COOKIE` wins over the saved one.
pub fn load_session(info: &PlatformInfo, store: &Store) -> Result<Loaded> {
  if let Ok(raw) = std::env::var(env_var(info))
    && !raw.trim().is_empty()
  {
    let cookies = browser::parse_cookie_string(&raw);
    let session = Session {
      cookies,
      source: Some("env".into()),
      ..Session::default()
    };
    return Ok(Loaded {
      session,
      from_env: true,
    });
  }
  Ok(Loaded {
    session: store.load_session()?.unwrap_or_default(),
    from_env: false,
  })
}

fn missing_cookie(info: &PlatformInfo, ctx: &Ctx) -> Option<&'static str> {
  info
    .required_cookies
    .iter()
    .copied()
    .find(|c| !ctx.http.has_cookie(c))
}

pub async fn login<P: Platform>(p: &P, args: LoginArgs) -> Result<Data> {
  let info = P::INFO;
  let ctx = p.ctx();
  // No browser holds a session of a platform without cookies.
  let cookieless = info.cookie_domains.is_empty();
  let use_qr = args.qrcode
    || (args.cookie.is_none()
      && args.browser.is_none()
      && args.token.is_none()
      && info.supports(Cap::QrLogin));
  let source = if let Some(raw) = args.token {
    token_login(p, raw).await?
  } else if let Some(raw) = args.cookie {
    cookie_login(ctx, &stdin_for_dash(raw)?)?
  } else if use_qr {
    qr_login(p).await?;
    "qrcode".to_owned()
  } else if cookieless || (args.browser.is_none() && !cfg!(feature = "browser")) {
    return Err(Error::input("choose how to log in").with_hint(ctx.login_hint()));
  } else {
    browser_login(ctx, args.browser)?
  };
  p.prepare_login().await?;
  if source != "token"
    && let Some(c) = missing_cookie(&info, ctx)
  {
    return Err(Error::auth(format!(
      "login incomplete: cookie `{c}` is missing"
    )));
  }
  let user = if source == "token" {
    p.token_whoami().await
  } else {
    p.whoami().await
  };
  let user = user.map_err(|e| refused(e, &source))?;
  let saved_at = Timestamp::now();
  let mut session = ctx.session();
  session.source = Some(source.clone());
  session.saved_at = Some(saved_at);
  ctx.store.save_session(&session)?;
  Ok(Data::Auth(AuthStatus {
    authenticated: true,
    user: Some(Box::new(user)),
    source: Some(source),
    saved_at: Some(saved_at),
    message: None,
  }))
}

/// `--token` (`-` reads it from stdin), handed to the platform.
async fn token_login<P: Platform>(p: &P, raw: String) -> Result<String> {
  if !P::INFO.supports(Cap::TokenLogin) {
    return Err(Error::unsupported("login --token").with_hint(p.ctx().login_hint()));
  }
  let token = stdin_for_dash(raw)?;
  if token.trim().is_empty() {
    return Err(Error::input("the token is empty"));
  }
  p.login_token(token.trim()).await?;
  Ok("token".to_owned())
}

/// A secret argument, or standard input for `-` (out of the shell history).
fn stdin_for_dash(raw: String) -> Result<String> {
  if raw == "-" {
    Ok(std::io::read_to_string(std::io::stdin())?)
  } else {
    Ok(raw)
  }
}

/// `--cookie 'name=value; ...'` (`-` reads it from stdin).
fn cookie_login(ctx: &Ctx, raw: &str) -> Result<String> {
  let cookies = browser::parse_cookie_string(raw);
  if cookies.is_empty() {
    return Err(Error::input(
      "the cookie string is empty; expected `name=value; name2=value2`",
    ));
  }
  ctx.http.replace_cookies(cookies);
  Ok("cookie".to_owned())
}

/// `--browser [NAME]`: the first local browser holding a complete session.
fn browser_login(ctx: &Ctx, wanted: Option<String>) -> Result<String> {
  let info = ctx.info;
  let wanted = wanted.filter(|b| b != "auto");
  let found = browser::import(wanted.as_deref(), info.cookie_domains)?;
  let pick = found.iter().find(|(_, jar)| {
    info
      .required_cookies
      .iter()
      .all(|c| jar.get(*c).is_some_and(|v| !v.is_empty()))
  });
  let Some((name, jar)) = pick else {
    let checked: Vec<&str> = found.iter().map(|(b, _)| b.as_str()).collect();
    let seen = if checked.is_empty() {
      "none had cookies".to_owned()
    } else {
      format!("found cookies in {}", checked.join(", "))
    };
    return Err(
      Error::auth(format!(
        "no logged-in {} session in local browsers ({seen})",
        info.name
      ))
      .with_hint(format!(
        "log in at {} in a browser first, or use `atp {} login --cookie '...'`",
        info.home, info.id
      )),
    );
  };
  ctx.http.replace_cookies(jar.clone());
  Ok(format!("browser:{name}"))
}

/// Say what was refused when the new session does not work.
fn refused(e: Error, source: &str) -> Error {
  if e.code != ErrorCode::NotAuthenticated {
    return e;
  }
  match source {
    "token" => e.with_hint("the platform did not accept this token"),
    "qrcode" => e,
    _ => e.with_hint("the platform did not accept these cookies"),
  }
}

async fn qr_login<P: Platform>(p: &P) -> Result<()> {
  let info = P::INFO;
  if !info.supports(Cap::QrLogin) {
    return Err(Error::unsupported("login --qrcode").with_hint(format!(
      "use `atp {} login --browser` or `--cookie '...'`",
      info.id
    )));
  }
  let ticket = p.qr_start().await?;
  let dir = p.ctx().store.cache_dir();
  let image = match &ticket.image {
    Some(bytes) => qr::show_image(bytes, dir)?,
    None => qr::show(&ticket.url, dir)?,
  };
  note(&format!(
    "Scan with the {} app and confirm. QR image: {}",
    info.name,
    image.display()
  ));
  let started = Instant::now();
  let mut scanned = false;
  while started.elapsed() < QR_TIMEOUT {
    tokio::time::sleep(QR_POLL).await;
    match p.qr_poll(&ticket).await? {
      QrStatus::Waiting => {}
      QrStatus::Scanned => {
        if !scanned {
          note("Scanned; confirm the login on your phone.");
          scanned = true;
        }
      }
      QrStatus::Confirmed => return Ok(()),
      QrStatus::Expired => return Err(Error::auth("the QR code expired; run login again")),
    }
  }
  Err(Error::auth(
    "timed out waiting for the QR code to be confirmed",
  ))
}

pub fn logout<P: Platform>(ctx: &Ctx) -> Result<Data> {
  let removed = ctx.store.clear_session()?;
  let message = if removed {
    "saved session removed"
  } else {
    "there was no saved session"
  };
  Ok(Data::Action(
    Action::done("logout", P::INFO.id).with_message(message),
  ))
}

pub async fn status<P: Platform>(p: &P, loaded: &Loaded) -> Result<(Data, ExitCode)> {
  let not_logged_in = |message: String| {
    let status = AuthStatus {
      message: Some(message),
      ..AuthStatus::default()
    };
    Ok((Data::Auth(status), ExitCode::FAILURE))
  };
  if !p.logged_in() {
    return not_logged_in(format!("no session; {}", p.ctx().login_hint()));
  }
  match p.whoami().await {
    Ok(user) => Ok((
      Data::Auth(AuthStatus {
        authenticated: true,
        user: Some(Box::new(user)),
        source: loaded.session.source.clone(),
        saved_at: loaded.session.saved_at,
        message: None,
      }),
      ExitCode::SUCCESS,
    )),
    Err(e) if e.code == ErrorCode::NotAuthenticated => {
      not_logged_in(format!("{}; {}", e.message, p.ctx().login_hint()))
    }
    Err(e) => Err(e),
  }
}

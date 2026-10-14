//! Login pages: `atp guide login` (the general topic plus every platform's
//! login ways) and `atp guide login <platform>`.

use super::sections;
use crate::platform::{PlatformInfo, PlatformKind, either};

/// What the binary knows about logging in to `info`, as a Markdown list:
/// the ways, the cookies a session needs, environment variables and what
/// works without an account.
pub fn summary(info: &PlatformInfo) -> String {
  let mut out = String::new();
  let ways = info.login_ways();
  if !ways.is_empty() {
    out.push_str(&format!("- Log in with {}.\n", either(&ways)));
  }
  if !info.required_cookies.is_empty() {
    let names = code_list(info.required_cookies);
    let noun = if names.len() > 1 { "cookies" } else { "cookie" };
    let names = match names.split_last() {
      Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
      _ => names.concat(),
    };
    out.push_str(&format!("- A session needs the {noun} {names}.\n"));
  }
  let mut env = Vec::new();
  if !info.cookie_domains.is_empty() {
    env.push(format!(
      "`{}` (a cookie header for one run, never saved)",
      cookie_env(info)
    ));
  }
  env.extend(code_list(info.env));
  if !env.is_empty() {
    out.push_str(&format!("- Environment: {}.\n", env.join(", ")));
  }
  if !info.guest.is_empty() {
    out.push_str(&format!("- Without an account: {}.\n", info.guest));
  }
  out
}

/// `atp guide login`: the general topic, then a table per kind of platform.
pub fn overview(platforms: &[PlatformInfo]) -> String {
  let mut out = super::topic("login")
    .map(|t| t.body.trim_end().to_owned())
    .unwrap_or_default();
  out.push_str(OVERVIEW_HEAD);
  for kind in PlatformKind::ALL {
    let rows: Vec<String> = platforms
      .iter()
      .filter(|p| p.kind == *kind)
      .map(row)
      .collect();
    if rows.is_empty() {
      continue;
    }
    out.push_str(&format!("\n### {}\n\n{TABLE_HEAD}", kind.title()));
    out.push_str(&rows.concat());
  }
  out
}

const OVERVIEW_HEAD: &str = "

## Every platform

How each platform logs in, the cookies a session needs, the environment
variables it reads besides `ATP_<ID>_COOKIE`, and what works without an
account. Details, steps and risks: `atp guide login <platform>`.
";

const TABLE_HEAD: &str = "\
| Platform | Ways | Session cookies | Environment | Without an account |
| --- | --- | --- | --- | --- |
";

fn row(info: &PlatformInfo) -> String {
  let cell = |items: Vec<String>| {
    if items.is_empty() {
      "—".to_owned()
    } else {
      items.join(", ")
    }
  };
  let guest = if info.guest.is_empty() {
    "—"
  } else {
    info.guest
  };
  format!(
    "| `{}` {} | {} | {} | {} | {} |\n",
    info.id,
    info.name,
    cell(info.login_words().iter().map(|w| (*w).to_owned()).collect()),
    cell(code_list(info.required_cookies)),
    cell(code_list(info.env)),
    guest.replace('|', "\\|"),
  )
}

/// `atp guide login <platform>`: the summary, then the platform's own
/// `## Logging in` section.
pub fn page(info: &PlatformInfo) -> String {
  let mut out = format!(
    "# Logging in to {} (`{}`)\n\n{}",
    info.name,
    info.id,
    summary(info)
  );
  match sections::section(info.guide, sections::LOGIN) {
    Some(body) => out.push_str(&format!("\n{}", sections::promote(body))),
    None => out.push_str(&format!(
      "\n{} has no notes on logging in yet.\n",
      info.name
    )),
  }
  out.push_str(&format!(
    "\n## Checking the session\n\n    atp {id} status      # exit code 0 when logged in\n    \
     atp {id} whoami\n    atp {id} logout      # forget the saved session\n\n\
     Captchas, error codes and where sessions live: `atp guide login`.\n",
    id = info.id
  ));
  out
}

fn cookie_env(info: &PlatformInfo) -> String {
  format!("ATP_{}_COOKIE", info.id.to_ascii_uppercase())
}

fn code_list(items: &[&str]) -> Vec<String> {
  items.iter().map(|i| format!("`{i}`")).collect()
}

//! The static description of a platform and the values its options take.

use std::time::Duration;

use super::Cap;

/// Platform-specific values accepted by shared options. An empty list hides the option.
#[derive(Debug, Clone, Copy)]
pub struct Choices {
  /// `search --sort`; the first entry is the default.
  pub search_sort: &'static [&'static str],
  /// `search --filter` (content type filters such as `video`, `image`).
  pub search_filter: &'static [&'static str],
  /// `hot --category`.
  pub hot_category: &'static [&'static str],
  /// `feed --type`.
  pub feed_kind: &'static [&'static str],
  /// `comments --sort`.
  pub comment_sort: &'static [&'static str],
  /// `notifications --type`.
  pub notification_kind: &'static [&'static str],
}

impl Choices {
  pub const NONE: Choices = Choices {
    search_sort: &[],
    search_filter: &[],
    hot_category: &[],
    feed_kind: &[],
    comment_sort: &[],
    notification_kind: &[],
  };
}

/// Static description of a platform.
#[derive(Debug, Clone, Copy)]
pub struct PlatformInfo {
  /// Subcommand name, e.g. `bili`.
  pub id: &'static str,
  /// Display name, e.g. `Bilibili`.
  pub name: &'static str,
  pub aliases: &'static [&'static str],
  /// One-line description for `--help`.
  pub about: &'static str,
  /// Home page; also the `Referer` for media downloads.
  pub home: &'static str,
  /// Cookie domains to read when importing from a browser.
  pub cookie_domains: &'static [&'static str],
  /// Cookies a logged-in session must contain.
  pub required_cookies: &'static [&'static str],
  pub caps: &'static [Cap],
  pub choices: Choices,
  /// Minimum gap between requests (anti-bot pacing); zero disables it.
  pub min_interval: Duration,
  /// Markdown notes appended to the generated `social guide <id>` topic:
  /// reference formats, login notes, analytics, limits.
  pub guide: &'static str,
}

impl PlatformInfo {
  pub fn supports(&self, cap: Cap) -> bool {
    self.caps.contains(&cap)
  }

  /// Every way to log in, as commands: `social <id> login` (QR code),
  /// `--token`, `--browser`, `--cookie`, as the platform offers them.
  pub fn login_ways(&self) -> Vec<String> {
    let id = self.id;
    let cookies = !self.cookie_domains.is_empty();
    let mut ways = Vec::new();
    if self.supports(Cap::QrLogin) {
      ways.push(format!("`social {id} login` (QR code)"));
    }
    if self.supports(Cap::TokenLogin) {
      ways.push(format!("`social {id} login --token '...'`"));
    }
    if cookies && cfg!(feature = "browser") {
      ways.push(format!("`social {id} login --browser`"));
    }
    if cookies {
      ways.push(format!("`social {id} login --cookie '...'`"));
    }
    ways
  }
}

/// `a`, `a or b`, `a, b or c`.
pub fn either(items: &[String]) -> String {
  match items.split_last() {
    None => String::new(),
    Some((last, [])) => last.clone(),
    Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
  }
}

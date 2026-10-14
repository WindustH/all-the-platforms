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
  /// The group `atp platforms` and the guide list it under.
  pub kind: PlatformKind,
  /// What `login --token` takes, as a placeholder (`BOT_TOKEN`,
  /// `APP_ID:APP_SECRET`); empty without [`Cap::TokenLogin`].
  pub token: &'static str,
  /// Environment variables with credentials or login settings, besides
  /// the `ATP_<ID>_COOKIE` every cookie platform reads.
  pub env: &'static [&'static str],
  /// What works without logging in, in a few words for `atp guide login`
  /// (`"search, read, comments, users, download"`, `"nothing"`).
  pub guest: &'static str,
  /// The crate's `GUIDE.md`: `## Logging in`, `## References`, notes per
  /// command under `## Commands` (`### search`, `### playlist create`),
  /// `## Limits`. `atp guide` shows it whole or section by section.
  pub guide: &'static str,
}

/// What a platform is for; groups `atp platforms` and the guide index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformKind {
  Social,
  Video,
  Chat,
  Shopping,
  Local,
  Drive,
  Music,
}

impl PlatformKind {
  pub const ALL: &[PlatformKind] = &[
    Self::Social,
    Self::Video,
    Self::Chat,
    Self::Shopping,
    Self::Local,
    Self::Drive,
    Self::Music,
  ];

  /// Lowercase name, as in `atp platforms --json`.
  pub fn name(self) -> &'static str {
    match self {
      Self::Social => "social",
      Self::Video => "video",
      Self::Chat => "chat",
      Self::Shopping => "shopping",
      Self::Local => "local",
      Self::Drive => "drive",
      Self::Music => "music",
    }
  }

  /// Heading for people.
  pub fn title(self) -> &'static str {
    match self {
      Self::Social => "Social and news",
      Self::Video => "Video",
      Self::Chat => "Chat",
      Self::Shopping => "Shopping",
      Self::Local => "Maps, local life and travel",
      Self::Drive => "Cloud drives",
      Self::Music => "Music",
    }
  }
}

impl PlatformInfo {
  pub fn supports(&self, cap: Cap) -> bool {
    self.caps.contains(&cap)
  }

  /// The ways to log in as single words: `QR`, `token`, `browser`, `cookie`.
  pub fn login_words(&self) -> Vec<&'static str> {
    let cookies = !self.cookie_domains.is_empty();
    [
      (self.supports(Cap::QrLogin), "QR"),
      (self.supports(Cap::TokenLogin), "token"),
      (cookies && cfg!(feature = "browser"), "browser"),
      (cookies, "cookie"),
    ]
    .into_iter()
    .filter_map(|(on, word)| on.then_some(word))
    .collect()
  }

  /// Every way to log in, as commands: `atp <id> login` (QR code),
  /// `--token`, `--browser`, `--cookie`, as the platform offers them.
  pub fn login_ways(&self) -> Vec<String> {
    let id = self.id;
    let cookies = !self.cookie_domains.is_empty();
    let mut ways = Vec::new();
    if self.supports(Cap::QrLogin) {
      ways.push(format!("`atp {id} login` (QR code)"));
    }
    if self.supports(Cap::TokenLogin) {
      let token = if self.token.is_empty() {
        "..."
      } else {
        self.token
      };
      ways.push(format!("`atp {id} login --token '{token}'`"));
    }
    if cookies && cfg!(feature = "browser") {
      ways.push(format!("`atp {id} login --browser`"));
    }
    if cookies {
      ways.push(format!("`atp {id} login --cookie '...'`"));
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

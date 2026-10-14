//! Options every command has, and the paging options of listings.

use jiff::Timestamp;

use crate::error::{Error, Result};
use crate::model::{Dated, Keyed, Page};
use crate::output::Format;
use crate::paging::{Bounds, Window, collect_window, collect_within};
use crate::platform::PageReq;

/// Options available on every command.
#[derive(Debug, Clone, clap::Args)]
pub struct GlobalArgs {
  /// Output format (default: table on a terminal, yaml when piped)
  #[arg(short = 'f', long, global = true, value_enum, env = "ATP_OUTPUT")]
  pub format: Option<Format>,
  /// Shortcut for `--format json` (wins over --format and ATP_OUTPUT)
  #[arg(long, global = true)]
  pub json: bool,
  /// Shortcut for `--format yaml` (wins over --format and ATP_OUTPUT)
  #[arg(long, global = true)]
  pub yaml: bool,
  /// Include the untouched upstream payloads (`raw` fields)
  #[arg(long, global = true)]
  pub raw: bool,
  /// Proxy URL (http, https or socks5); HTTPS_PROXY / ALL_PROXY are honored too
  #[arg(long, global = true, env = "ATP_PROXY")]
  pub proxy: Option<String>,
  /// Request timeout in seconds
  #[arg(long, global = true, default_value_t = 30)]
  pub timeout: u64,
  /// Minimum seconds between requests (random jitter is added); slows bulk
  /// jobs down, never below the platform's own pacing
  #[arg(long, global = true, env = "ATP_INTERVAL", value_name = "SECS")]
  pub interval: Option<f64>,
  /// Log requests to stderr
  #[arg(short, long, global = true)]
  pub verbose: bool,
  /// Fail on captchas and risk checks instead of opening them in a browser
  /// (ATP_NO_VERIFY takes 1/0, true/false, yes/no, on/off)
  #[arg(
    long,
    global = true,
    env = "ATP_NO_VERIFY",
    value_parser = clap::builder::BoolishValueParser::new(),
    action = clap::ArgAction::SetTrue
  )]
  pub no_verify: bool,
}

impl GlobalArgs {
  pub fn output_format(&self) -> Format {
    Format::resolve(if self.json {
      Some(Format::Json)
    } else if self.yaml {
      Some(Format::Yaml)
    } else {
      self.format
    })
  }
}

#[derive(Debug, clap::Args)]
pub struct PageArgs {
  /// Number of items to fetch; pages are followed automatically
  #[arg(short = 'n', long, default_value_t = 20)]
  pub limit: usize,
  /// Continue from the `next_cursor` of an earlier listing
  #[arg(long)]
  pub cursor: Option<String>,
  /// Only items published since then: 7d, 12h, 2026-09-01 or an RFC 3339 time
  #[arg(long, value_parser = crate::text::parse_when, value_name = "WHEN")]
  pub since: Option<Timestamp>,
  /// Only items published until then (same forms as --since)
  #[arg(long, value_parser = crate::text::parse_when, value_name = "WHEN")]
  pub until: Option<Timestamp>,
  /// Stop after N upstream pages; the cursor continues from there
  #[arg(long, value_name = "N", value_parser = clap::value_parser!(u32).range(1..))]
  pub max_pages: Option<u32>,
}

impl PageArgs {
  pub fn window(&self) -> Window {
    Window {
      since: self.since,
      until: self.until,
    }
  }

  /// `--limit` and `--max-pages`.
  pub fn bounds(&self) -> Bounds {
    Bounds {
      pages: self.max_pages.map(|n| n as usize),
      ..Bounds::items(self.limit)
    }
  }

  /// Follow a listing of dated items (posts, comments, notifications) within `--since` / `--until`.
  pub async fn collect_dated<T: Dated + Keyed>(
    &self,
    fetch: impl AsyncFnMut(PageReq) -> Result<Page<T>>,
  ) -> Result<Page<T>> {
    collect_window(self.bounds(), self.cursor.clone(), self.window(), fetch).await
  }

  /// Follow a listing without publication times (users, collections).
  pub async fn collect<T: Keyed>(
    &self,
    fetch: impl AsyncFnMut(PageReq) -> Result<Page<T>>,
  ) -> Result<Page<T>> {
    if !self.window().is_open() {
      return Err(Error::input(
        "--since / --until only apply to posts, comments and notifications",
      ));
    }
    collect_within(self.bounds(), self.cursor.clone(), fetch).await
  }
}

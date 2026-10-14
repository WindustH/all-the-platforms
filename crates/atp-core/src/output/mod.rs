//! Printing results: a human view for terminals, a stable envelope
//! (`{ok, schema_version, platform, fetched_at, data | error}`) as JSON or
//! YAML for scripts and agents, and flat rows (JSON Lines, CSV) for analysis.
//! Non-terminal stdout defaults to YAML.

mod human;
pub(crate) mod rows;

use std::io::{IsTerminal, Write};

use serde::Serialize;

use crate::error::Error;
use crate::model::Data;

pub const SCHEMA_VERSION: &str = "1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Format {
  /// Tables and cards for people.
  Table,
  Json,
  Yaml,
  /// One JSON object per item, no envelope (errors go to stderr).
  Jsonl,
  /// One row per item with dotted columns, no envelope (errors go to stderr).
  Csv,
}

impl Format {
  /// Explicit choice, else `table` on a terminal and `yaml` when piped.
  pub fn resolve(explicit: Option<Format>) -> Format {
    explicit.unwrap_or_else(|| {
      if std::io::stdout().is_terminal() {
        Format::Table
      } else {
        Format::Yaml
      }
    })
  }
}

#[derive(Serialize)]
struct Envelope<'a, T: Serialize> {
  ok: bool,
  schema_version: &'static str,
  #[serde(skip_serializing_if = "Option::is_none")]
  platform: Option<&'a str>,
  fetched_at: jiff::Timestamp,
  #[serde(skip_serializing_if = "Option::is_none")]
  data: Option<&'a T>,
  #[serde(skip_serializing_if = "Option::is_none")]
  error: Option<&'a Error>,
}

fn print_structured<T: Serialize>(format: Format, envelope: &Envelope<'_, T>) {
  let text = match format {
    Format::Json => serde_json::to_string_pretty(envelope).unwrap_or_default() + "\n",
    _ => serde_saphyr::to_string(envelope).unwrap_or_default(),
  };
  let mut out = std::io::stdout().lock();
  let _ = out.write_all(text.as_bytes());
  let _ = out.flush();
}

/// Rows with `platform` and `fetched_at` added, for repeated snapshots. Rows
/// hold items only: where a listing continues goes to stderr, as in tables.
fn print_rows(format: Format, platform: Option<&str>, data: &Data) {
  write_rows(format, platform, data);
  if let Some(cursor) = data.next_cursor() {
    note(&format!("more: --cursor {cursor}"));
  }
}

fn write_rows(format: Format, platform: Option<&str>, data: &Data) {
  let fetched_at = jiff::Timestamp::now().to_string();
  let rows: Vec<serde_json::Map<String, serde_json::Value>> = rows::rows(data)
    .iter()
    .map(|r| {
      let mut row = match format {
        Format::Csv => rows::flatten(r),
        _ => match r {
          serde_json::Value::Object(m) => m.clone(),
          other => serde_json::Map::from_iter([("value".to_owned(), other.clone())]),
        },
      };
      if let Some(p) = platform {
        row.insert("platform".into(), p.into());
      }
      row.insert("fetched_at".into(), fetched_at.clone().into());
      row
    })
    .collect();
  let mut out = std::io::stdout().lock();
  if format == Format::Jsonl {
    for row in &rows {
      let _ = writeln!(out, "{}", serde_json::Value::Object(row.clone()));
    }
    let _ = out.flush();
    return;
  }
  // Columns in first-seen order across all rows.
  let mut columns: Vec<&String> = Vec::new();
  for row in &rows {
    for key in row.keys() {
      if !columns.contains(&key) {
        columns.push(key);
      }
    }
  }
  let mut w = csv::Writer::from_writer(out);
  let _ = w.write_record(columns.iter().map(|c| c.as_str()));
  for row in &rows {
    let _ = w.write_record(
      columns
        .iter()
        .map(|c| row.get(*c).map(rows::cell).unwrap_or_default()),
    );
  }
  let _ = w.flush();
}

/// Print a successful result.
pub fn emit(format: Format, platform: Option<&str>, data: &Data) {
  match format {
    Format::Table => human::render(data),
    Format::Jsonl | Format::Csv => print_rows(format, platform, data),
    _ => print_structured(
      format,
      &Envelope {
        ok: true,
        schema_version: SCHEMA_VERSION,
        platform,
        fetched_at: jiff::Timestamp::now(),
        data: Some(data),
        error: None,
      },
    ),
  }
}

/// Print a failure: the envelope on stdout for machines, a message on stderr for people.
pub fn emit_error(format: Format, platform: Option<&str>, error: &Error) {
  match format {
    Format::Table | Format::Jsonl | Format::Csv => human::error(error),
    _ => print_structured::<()>(
      format,
      &Envelope {
        ok: false,
        schema_version: SCHEMA_VERSION,
        platform,
        fetched_at: jiff::Timestamp::now(),
        data: None,
        error: Some(error),
      },
    ),
  }
}

thread_local! {
  /// Events of live commands collected instead of printed (MCP server).
  static CAPTURED: std::cell::RefCell<Option<Vec<crate::model::Post>>> =
    const { std::cell::RefCell::new(None) };
}

/// From now on, collect live events instead of printing them.
pub(crate) fn capture() {
  CAPTURED.set(Some(Vec::new()));
}

/// The events collected since [`capture`]; printing resumes.
pub(crate) fn captured() -> Vec<crate::model::Post> {
  CAPTURED.take().unwrap_or_default()
}

/// The envelope of a result as JSON, the way `--json` prints it.
pub(crate) fn envelope(platform: Option<&str>, result: &Result<Data, Error>) -> serde_json::Value {
  let (data, error) = match result {
    Ok(data) => (Some(data), None),
    Err(e) => (None, Some(e)),
  };
  serde_json::to_value(Envelope {
    ok: error.is_none(),
    schema_version: SCHEMA_VERSION,
    platform,
    fetched_at: jiff::Timestamp::now(),
    data,
    error,
  })
  .unwrap_or_default()
}

/// Live output of `listen`: each event is printed the moment it arrives, as
/// a line for people, one JSON object (`jsonl`, `json`) or YAML document
/// (`yaml`) per event, or a CSV row of [`LIVE_COLUMNS`] under one header,
/// with `platform` and `fetched_at` added.
pub struct Stream {
  format: Format,
  platform: &'static str,
  raw: bool,
  /// The CSV header is out.
  header: std::sync::atomic::AtomicBool,
}

/// The columns of `listen -f csv`: events come one by one, so the columns
/// are fixed up front (`jsonl` keeps every field).
pub const LIVE_COLUMNS: &[&str] = &[
  "platform",
  "fetched_at",
  "created_at",
  "kind",
  "id",
  "extra.chat",
  "extra.chat_name",
  "author.id",
  "author.name",
  "author.handle",
  "title",
  "text",
  "url",
  "media",
];

impl Stream {
  /// `raw`: keep the upstream payloads (`--raw`).
  pub fn new(format: Format, platform: &'static str, raw: bool) -> Self {
    Self {
      format,
      platform,
      raw,
      header: std::sync::atomic::AtomicBool::new(false),
    }
  }

  pub fn post(&self, post: &crate::model::Post) {
    if CAPTURED.with_borrow(Option::is_some) {
      let mut post = post.clone();
      if !self.raw {
        post.raw = None;
      }
      CAPTURED.with_borrow_mut(|c| c.get_or_insert_default().push(post));
      return;
    }
    if !self.raw && post.raw.is_some() {
      let mut post = post.clone();
      post.raw = None;
      return self.print(&post);
    }
    self.print(post);
  }

  fn print(&self, post: &crate::model::Post) {
    if self.format == Format::Table {
      human::stream_line(post);
      return;
    }
    let mut row = match serde_json::to_value(post) {
      Ok(serde_json::Value::Object(m)) => m,
      _ => return,
    };
    row.insert("platform".into(), self.platform.into());
    row.insert(
      "fetched_at".into(),
      jiff::Timestamp::now().to_string().into(),
    );
    let row = serde_json::Value::Object(row);
    let text = match self.format {
      Format::Yaml => format!("---\n{}", serde_saphyr::to_string(&row).unwrap_or_default()),
      Format::Csv => self.csv_row(&row),
      _ => format!("{row}\n"),
    };
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(text.as_bytes());
    let _ = out.flush();
  }

  /// One event as a CSV row of [`LIVE_COLUMNS`], after the header the first time.
  fn csv_row(&self, row: &serde_json::Value) -> String {
    let flat = rows::flatten(row);
    let mut w = csv::Writer::from_writer(Vec::new());
    if !self.header.swap(true, std::sync::atomic::Ordering::Relaxed) {
      let _ = w.write_record(LIVE_COLUMNS);
    }
    let cells = LIVE_COLUMNS
      .iter()
      .map(|c| flat.get(*c).map(rows::cell).unwrap_or_default());
    let _ = w.write_record(cells);
    String::from_utf8(w.into_inner().unwrap_or_default()).unwrap_or_default()
  }
}

/// A byte-counting progress line on stderr, labelled `label`. Until
/// [`set_total`] gives the size it is a spinner with the bytes so far, so
/// downloads of unknown size show no made-up total.
pub(crate) fn progress(label: &str) -> indicatif::ProgressBar {
  let bar = indicatif::ProgressBar::no_length();
  bar.set_style(style("{spinner} {msg:38!} {bytes:>10} {bytes_per_sec:>12}"));
  bar.set_message(label.to_owned());
  bar.enable_steady_tick(std::time::Duration::from_millis(120));
  bar
}

/// The size became known: from now on a bar with the total.
pub(crate) fn set_total(bar: &indicatif::ProgressBar, total: u64) {
  bar.set_style(style(
    "{msg:40!} {bytes:>10}/{total_bytes:10} {bar:24} {bytes_per_sec:>10}",
  ));
  bar.set_length(total);
}

fn style(template: &str) -> indicatif::ProgressStyle {
  indicatif::ProgressStyle::with_template(template)
    .unwrap_or_else(|_| indicatif::ProgressStyle::default_bar())
}

/// Progress / status line for people, on stderr so it never mixes with data.
pub fn note(message: &str) {
  human::note(message);
}

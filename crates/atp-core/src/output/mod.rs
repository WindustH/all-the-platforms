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

/// Rows with `platform` and `fetched_at` added, for repeated snapshots.
fn print_rows(format: Format, platform: Option<&str>, data: &Data) {
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
/// a line for people or one JSON object (`jsonl`, `json`, `csv`) or YAML
/// document (`yaml`) per event, with `platform` and `fetched_at` added.
pub struct Stream {
  format: Format,
  platform: &'static str,
  raw: bool,
}

impl Stream {
  /// `raw`: keep the upstream payloads (`--raw`).
  pub fn new(format: Format, platform: &'static str, raw: bool) -> Self {
    Self {
      format,
      platform,
      raw,
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
      _ => format!("{row}\n"),
    };
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(text.as_bytes());
    let _ = out.flush();
  }
}

/// A byte-counting progress bar on stderr, labelled `label`; its length is
/// set once known.
pub(crate) fn progress(label: &str) -> indicatif::ProgressBar {
  let bar = indicatif::ProgressBar::new(0);
  bar.set_style(
    indicatif::ProgressStyle::with_template(
      "{msg:40!} {bytes:>10}/{total_bytes:10} {bar:24} {bytes_per_sec:>10}",
    )
    .unwrap_or_else(|_| indicatif::ProgressStyle::default_bar()),
  );
  bar.set_message(label.to_owned());
  bar
}

/// Progress / status line for people, on stderr so it never mixes with data.
pub fn note(message: &str) {
  human::note(message);
}

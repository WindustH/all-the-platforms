//! Checks and conversions of command arguments shared by every area.

use std::io::{IsTerminal, Read};
use std::path::PathBuf;

use crate::error::{Error, Result};

/// Ask before something irreversible, unless `--yes`; refuse when nobody
/// can answer (stdin is not a terminal).
pub fn confirm(yes: bool, what: &str) -> Result<()> {
  if yes {
    return Ok(());
  }
  if !std::io::stdin().is_terminal() {
    return Err(Error::input(format!("refusing to {what} without --yes")));
  }
  eprint!("{what}? [y/N] ");
  let mut answer = String::new();
  std::io::stdin().read_line(&mut answer)?;
  if matches!(answer.trim(), "y" | "Y" | "yes") {
    Ok(())
  } else {
    Err(Error::input("cancelled"))
  }
}

/// What an `edit` changes, shown before it goes out: a title line, then
/// one row per field, `old → new` or the value with `(unchanged)`. Every
/// platform's edit prints it the same way, then asks with [`confirm`].
#[derive(Debug, Clone)]
pub struct Change {
  lines: Vec<String>,
}

impl Change {
  /// A summary headed `title` (`Douyin video 7697611474302225714:`).
  pub fn new(title: impl Into<String>) -> Self {
    Self {
      lines: vec![title.into()],
    }
  }

  /// A field that may change. Values show on one line, cut to a short
  /// width; an empty one reads `(none)`.
  pub fn field(&mut self, label: &str, old: &str, new: &str) -> &mut Self {
    let value = if old == new {
      format!("{} (unchanged)", shown(new))
    } else {
      format!("{} → {}", shown(old), shown(new))
    };
    self.row(label, &value)
  }

  /// A row with a value as it is: a file to upload, the edits left.
  pub fn row(&mut self, label: &str, value: &str) -> &mut Self {
    self.lines.push(format!("  {label:<11} {value}"));
    self
  }

  /// Print the summary, then ask `what?` (or take `--yes`).
  pub fn confirm(&self, yes: bool, what: &str) -> Result<()> {
    crate::output::note(&self.lines.join("\n"));
    confirm(yes, what)
  }
}

fn shown(value: &str) -> String {
  let line = crate::text::one_line(value);
  if line.is_empty() {
    "(none)".to_owned()
  } else {
    crate::text::truncate(&line, 60)
  }
}

/// A text argument, or standard input for `-`.
pub fn read_text(text: Option<String>) -> Result<String> {
  match text.as_deref() {
    Some("-") => {
      let mut buf = String::new();
      std::io::stdin().read_to_string(&mut buf)?;
      Ok(buf.trim_end().to_owned())
    }
    Some(t) => Ok(t.to_owned()),
    None => Ok(String::new()),
  }
}

/// Fails on the first image path that is not a file.
pub fn check_images(images: &[PathBuf]) -> Result<()> {
  missing("image", images)
}

/// Fails on the first attachment path that is not a file.
pub fn check_files(files: &[PathBuf]) -> Result<()> {
  missing("file", files)
}

fn missing(what: &str, paths: &[PathBuf]) -> Result<()> {
  match paths.iter().find(|p| !p.is_file()) {
    Some(path) => Err(Error::input(format!(
      "{what} not found: {}",
      path.display()
    ))),
    None => Ok(()),
  }
}

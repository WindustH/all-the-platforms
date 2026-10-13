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

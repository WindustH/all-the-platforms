//! Printing a guide page: lightly styled on a terminal, verbatim otherwise.

use std::io::IsTerminal;

use owo_colors::{OwoColorize, Stream};

/// Print Markdown: lightly styled on a terminal, verbatim otherwise.
pub fn print(markdown: &str) {
  if !std::io::stdout().is_terminal() {
    print!("{markdown}");
    return;
  }
  let mut fenced = false;
  // An inline `code` span may continue on the next line of a paragraph.
  let mut open = false;
  for line in markdown.lines() {
    if line.trim().is_empty() || line.starts_with('#') {
      open = false;
    }
    if line.starts_with("```") {
      fenced = !fenced;
      continue;
    }
    if fenced || line.starts_with("    ") {
      let code = line.strip_prefix("    ").unwrap_or(line);
      println!(
        "    {}",
        code.if_supports_color(Stream::Stdout, |t| t.cyan())
      );
    } else if let Some(h) = line.strip_prefix("# ").map(|h| h.replace('`', "")) {
      println!(
        "{}",
        h.if_supports_color(Stream::Stdout, |t| t.bold())
          .if_supports_color(Stream::Stdout, |t| t.underline())
      );
    } else if let Some(h) = line
      .strip_prefix("## ")
      .or_else(|| line.strip_prefix("### "))
      .map(|h| h.replace('`', ""))
    {
      println!("{}", h.if_supports_color(Stream::Stdout, |t| t.bold()));
    } else {
      println!("{}", inline(line, &mut open));
    }
  }
}

/// `code` in cyan and **bold** in bold; the markers themselves are
/// dropped. `open` says whether a code span from an earlier line goes on.
fn inline(line: &str, open: &mut bool) -> String {
  let mut out = String::new();
  for (i, part) in line.split('`').enumerate() {
    if i > 0 {
      *open = !*open;
    }
    if *open {
      out.push_str(
        &part
          .if_supports_color(Stream::Stdout, |t| t.cyan())
          .to_string(),
      );
      continue;
    }
    for (j, bit) in part.split("**").enumerate() {
      if j % 2 == 1 {
        out.push_str(
          &bit
            .if_supports_color(Stream::Stdout, |t| t.bold())
            .to_string(),
        );
      } else {
        out.push_str(bit);
      }
    }
  }
  out
}

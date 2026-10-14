//! Drive entries: a table of files, folders or shares, and one entry as a card.

use crate::model::{FileEntry, ShareLink};
use crate::text::fmt_size;

use super::*;

pub(super) fn file_table(files: &[FileEntry]) {
  let sizes = files.iter().any(|f| f.size.is_some());
  let shares = files.iter().any(|f| f.share.is_some());
  let mut header = vec!["#", "Name"];
  header.extend(sizes.then_some("Size"));
  header.push("Modified");
  header.extend(shares.then_some("Link"));
  header.push("ID");
  let mut t = table(&header);
  for (i, f) in files.iter().enumerate() {
    let mut row = vec![index(i), Cell::new(truncate(&f.label(), 60))];
    if sizes {
      let size = f.size.map(fmt_size).unwrap_or_default();
      row.push(Cell::new(size).set_alignment(CellAlignment::Right));
    }
    row.push(Cell::new(f.updated_at.map(fmt_time).unwrap_or_default()));
    if shares {
      row.push(Cell::new(
        f.share.as_ref().map(link_label).unwrap_or_default(),
      ));
    }
    row.push(Cell::new(&f.id).add_attribute(Attribute::Dim));
    t.add_row(row);
  }
  println!("{t}");
}

pub(super) fn file_card(f: &FileEntry) {
  println!(
    "{}",
    f.label().if_supports_color(Stream::Stdout, |t| t.bold())
  );
  let mut meta = vec![if f.dir { "folder" } else { "file" }.to_owned()];
  meta.extend(f.size.map(fmt_size));
  meta.extend(f.updated_at.map(fmt_time));
  meta.push(format!("id {}", f.id));
  println!(
    "{}",
    meta
      .join(" · ")
      .if_supports_color(Stream::Stdout, |t| t.dimmed())
  );
  if let Some(md5) = &f.md5 {
    println!("md5 {md5}");
  }
  if let Some(share) = &f.share {
    println!(
      "{}",
      link_label(share).if_supports_color(Stream::Stdout, |t| t.cyan())
    );
    if let Some(t) = share.expires_at {
      note(&format!("expires {}", fmt_time(t)));
    }
  }
  for (key, value) in &f.extra {
    let value = match value {
      serde_json::Value::String(s) => s.clone(),
      other => other.to_string(),
    };
    println!(
      "{} {}",
      format!("{key}:").if_supports_color(Stream::Stdout, |t| t.dimmed()),
      value
    );
  }
}

/// `https://… (code abcd)`.
fn link_label(share: &ShareLink) -> String {
  match &share.code {
    Some(code) => format!("{} (code {code})", share.url),
    None => share.url.clone(),
  }
}

//! Terminal rendering of [`Data`] for people.

mod cards;
mod files;
mod geo;
mod insights;
mod tables;

use crate::error::Error;
use crate::model::{
  Action, AuthStatus, Collection, Comment, Data, Downloaded, Insights, Metrics, Notification, Page,
  Post, Price, Transcript, User,
};
use crate::text::{fmt_count, fmt_duration, fmt_time, one_line, truncate};
use comfy_table::{Attribute, Cell, CellAlignment, ContentArrangement, Table, presets};
use owo_colors::{OwoColorize, Stream};

pub use self::cards::stream_line;
use self::cards::*;
use self::files::*;
use self::geo::*;
use self::insights::*;
use self::tables::*;

pub fn render(data: &Data) {
  match data {
    Data::Posts(page) => list(page, post_table),
    Data::Users(page) => list(page, user_table),
    Data::Comments(page) => list(page, comment_tree),
    Data::Collections(page) => list(page, collection_table),
    Data::Notifications(page) => list(page, notification_table),
    Data::Files(page) => list(page, file_table),
    Data::Post(post) => post_card(post),
    Data::User(user) => user_card(user),
    Data::File(file) => file_card(file),
    Data::Action(action) => action_line(action),
    Data::Actions(actions) => action_lines(actions),
    Data::Auth(auth) => auth_line(auth),
    Data::Counts(counts) => {
      for (k, v) in counts {
        println!(
          "{:<16} {}",
          k.if_supports_color(Stream::Stdout, |t| t.dimmed()),
          v
        );
      }
    }
    Data::Transcript(t) => transcript(t),
    Data::Downloads(files) => downloads(files),
    Data::Insights(i) => insights(i),
    Data::Routes(r) => routes(r),
    Data::Distances(list) => distances(list),
    Data::Value(v) => print!("{}", serde_saphyr::to_string(v).unwrap_or_default()),
  }
}

pub fn error(e: &Error) {
  eprintln!(
    "{} {}",
    "error:".if_supports_color(Stream::Stderr, |t| t.red()),
    e.message
  );
  if let Some(hint) = &e.hint {
    eprintln!(
      "{} {}",
      "hint:".if_supports_color(Stream::Stderr, |t| t.yellow()),
      hint
    );
  }
  if let Some(url) = &e.url
    && !e.hint.as_deref().unwrap_or_default().contains(url.as_str())
  {
    eprintln!(
      "{} {url}",
      "open:".if_supports_color(Stream::Stderr, |t| t.yellow())
    );
  }
}

pub fn note(message: &str) {
  eprintln!(
    "{}",
    message.if_supports_color(Stream::Stderr, |t| t.dimmed())
  );
}

fn list<T>(page: &Page<T>, draw: fn(&[T])) {
  if page.items.is_empty() {
    note("(no results)");
  } else {
    draw(&page.items);
  }
  if let Some(cursor) = &page.next_cursor {
    note(&format!("more: --cursor {cursor}"));
  }
}

fn table(header: &[&str]) -> Table {
  let mut t = Table::new();
  t.load_style(presets::NOTHING)
    .set_content_arrangement(ContentArrangement::Dynamic);
  t.set_header(
    header
      .iter()
      .map(|h| Cell::new(h).add_attribute(Attribute::Bold)),
  );
  t
}

fn num(n: Option<u64>) -> Cell {
  Cell::new(n.map(fmt_count).unwrap_or_default()).set_alignment(CellAlignment::Right)
}

fn index(i: usize) -> Cell {
  Cell::new(i + 1).add_attribute(Attribute::Dim)
}

fn user_label(u: &User) -> String {
  match &u.handle {
    Some(h) if h != &u.name && !h.is_empty() => format!("{} @{}", u.name, h),
    _ => u.name.clone(),
  }
}

fn headline(p: &Post) -> String {
  let title = p.title.as_deref().map(one_line).unwrap_or_default();
  let text = p.text.as_deref().map(one_line).unwrap_or_default();
  let s = if title.is_empty() { text } else { title };
  truncate(&s, 80)
}

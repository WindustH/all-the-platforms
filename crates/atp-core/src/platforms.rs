//! `atp platforms`: every platform on one row, grouped by kind, or one
//! platform in detail (`atp platforms bili`). Structured formats carry
//! the same data as JSON.

use std::process::ExitCode;

use clap::Subcommand;
use comfy_table::{
  Attribute, Cell, CellAlignment, ColumnConstraint, ContentArrangement, Table, presets,
};
use owo_colors::{OwoColorize, Stream};
use serde_json::{Value, json};

use crate::app::{App, Entry};
use crate::cli::CommonCommand;
use crate::model::Data;
use crate::output::{self, Format};
use crate::platform::PlatformKind;

/// Every platform with its capabilities, own commands and logins.
pub(crate) fn value(app: &App) -> Value {
  let list: Vec<Value> = app.platforms().iter().map(entry_value).collect();
  json!({ "platforms": list })
}

fn entry_value(e: &Entry) -> Value {
  let info = &e.info;
  json!({
    "id": info.id,
    "name": info.name,
    "aliases": info.aliases,
    "home": info.home,
    "kind": info.kind.name(),
    "capabilities": info.caps.iter().map(|c| c.name()).collect::<Vec<_>>(),
    "extra_commands": (e.extras)(),
    "login": {
      "ways": info.login_words(),
      "token": (!info.token.is_empty()).then_some(info.token),
      "required_cookies": info.required_cookies,
      "env": info.env,
      "guest": (!info.guest.is_empty()).then_some(info.guest),
    },
  })
}

/// `atp platforms [ID]`.
pub(crate) fn print(app: &App, id: Option<&str>, format: Format) -> ExitCode {
  let Some(id) = id else {
    if format == Format::Table {
      table(app);
    } else {
      output::emit(format, None, &Data::Value(value(app)));
    }
    return ExitCode::SUCCESS;
  };
  let Some(entry) = app.find(id) else {
    let names: Vec<&str> = app
      .platforms()
      .iter()
      .flat_map(|e| std::iter::once(e.info.id).chain(e.info.aliases.iter().copied()))
      .collect();
    eprintln!(
      "error: no platform `{id}`.{} `atp platforms` lists them.",
      crate::guide::did_you_mean(id, &names)
    );
    return ExitCode::from(2);
  };
  if format == Format::Table {
    detail(entry);
  } else {
    output::emit(
      format,
      Some(entry.info.id),
      &Data::Value(entry_value(entry)),
    );
  }
  ExitCode::SUCCESS
}

/// One row per platform, under a row naming its kind.
fn table(app: &App) {
  let mut t = Table::new();
  t.load_style(presets::NOTHING)
    .set_content_arrangement(ContentArrangement::Dynamic);
  t.set_header(
    ["ID", "NAME", "ALIASES", "LOGIN", "SHARED", "OWN"]
      .map(|h| Cell::new(h).add_attribute(Attribute::Bold)),
  );
  // On narrow terminals names and aliases wrap; ids, logins and counts stay whole.
  for i in [0, 3, 4, 5] {
    if let Some(column) = t.column_mut(i) {
      column.set_constraint(ColumnConstraint::ContentWidth);
    }
  }
  for kind in PlatformKind::ALL {
    let members: Vec<&Entry> = app
      .platforms()
      .iter()
      .filter(|e| e.info.kind == *kind)
      .collect();
    if members.is_empty() {
      continue;
    }
    // The short name keeps the first column as narrow as the ids.
    t.add_row(vec![
      Cell::new(kind.name().to_uppercase()).add_attribute(Attribute::Italic),
    ]);
    for e in members {
      let (shared, own) = commands(e);
      let count = |n: usize| Cell::new(n).set_alignment(CellAlignment::Right);
      t.add_row(vec![
        Cell::new(e.info.id),
        Cell::new(e.info.name),
        Cell::new(e.info.aliases.join(", ")),
        Cell::new(e.info.login_words().join(" ")),
        count(shared.len()),
        count(own.len()),
      ]);
    }
  }
  println!("{t}");
  println!(
    "\nSHARED and OWN count the shared and the platform's own commands. One \
     platform in detail: `atp platforms <id>` or `atp guide <id>`."
  );
}

/// The visible shared and own commands of a platform, in `--help` order.
fn commands(e: &Entry) -> (Vec<String>, Vec<String>) {
  let cmd = (e.command)();
  let mut list: Vec<&clap::Command> = cmd
    .get_subcommands()
    .filter(|c| !c.is_hide_set() && c.get_name() != "help")
    .collect();
  list.sort_by_key(|c| c.get_display_order());
  let (shared, own): (Vec<_>, Vec<_>) = list
    .into_iter()
    .map(|c| c.get_name().to_owned())
    .partition(|n| CommonCommand::has_subcommand(n));
  (shared, own)
}

/// `atp platforms <id>` on a terminal.
fn detail(e: &Entry) {
  let info = &e.info;
  let title = format!("{} ({})", info.name, info.id);
  println!("{}", title.if_supports_color(Stream::Stdout, |t| t.bold()));
  println!("{}", info.about);
  let mut lines = vec![
    ("Kind", info.kind.title().to_owned()),
    ("Home", info.home.to_owned()),
  ];
  if !info.aliases.is_empty() {
    lines.push(("Aliases", info.aliases.join(", ")));
  }
  lines.push(("Login", info.login_words().join(", ")));
  if !info.required_cookies.is_empty() {
    lines.push(("Cookies", info.required_cookies.join(", ")));
  }
  if !info.env.is_empty() {
    lines.push(("Environment", info.env.join(", ")));
  }
  if !info.guest.is_empty() {
    lines.push(("No account", info.guest.to_owned()));
  }
  let (shared, own) = commands(e);
  lines.push(("Shared", shared.join(", ")));
  if !own.is_empty() {
    lines.push(("Only here", own.join(", ")));
  }
  println!();
  for (label, text) in lines {
    let label = format!("{label:<12}");
    println!(
      "{} {text}",
      label.if_supports_color(Stream::Stdout, |t| t.dimmed())
    );
  }
  println!(
    "\nGuide: `atp guide {id}`, one command: `atp guide {id} <command>`, logging in: \
     `atp guide login {id}`.",
    id = info.id
  );
}

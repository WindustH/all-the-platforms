//! The top-level program: a registry of platforms behind one `atp` command.
//!
//! The binary only lists the platforms; everything else (argument parsing,
//! runtime, output) happens here. Invoking the binary through a symlink named
//! after a platform (`bili`, `xhs`, ...) selects that platform directly.

use std::ffi::OsString;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::process::ExitCode;

use clap::{ArgMatches, Args, FromArgMatches};
use comfy_table::{Attribute, Cell, CellAlignment, ContentArrangement, Table, presets};
use serde_json::json;

use crate::cli::{self, GlobalArgs};
use crate::guide;
use crate::model::Data;
use crate::output::{self, Format};
use crate::platform::{Cap, Platform, PlatformInfo};

type RunFuture = Pin<Box<dyn Future<Output = ExitCode>>>;
pub(crate) type CallFuture = Pin<Box<dyn Future<Output = crate::error::Result<Data>>>>;

pub(crate) struct Entry {
  pub info: PlatformInfo,
  pub command: fn() -> clap::Command,
  extras: fn() -> Vec<String>,
  run: fn(GlobalArgs, ArgMatches) -> RunFuture,
  /// Run without printing: the data or the error, for the MCP server.
  pub call: fn(GlobalArgs, ArgMatches) -> CallFuture,
}

pub struct App {
  name: &'static str,
  pub(crate) version: &'static str,
  about: &'static str,
  platforms: Vec<Entry>,
}

fn run_boxed<P: Platform + 'static>(global: GlobalArgs, matches: ArgMatches) -> RunFuture {
  Box::pin(cli::run::<P>(global, matches))
}

fn call_boxed<P: Platform + 'static>(global: GlobalArgs, matches: ArgMatches) -> CallFuture {
  Box::pin(cli::call::<P>(global, matches))
}

impl App {
  pub fn new(name: &'static str, version: &'static str, about: &'static str) -> Self {
    Self {
      name,
      version,
      about,
      platforms: Vec::new(),
    }
  }

  pub fn platform<P: Platform + 'static>(mut self) -> Self {
    self.platforms.push(Entry {
      info: P::INFO,
      command: cli::command::<P>,
      extras: cli::extra_commands::<P>,
      run: run_boxed::<P>,
      call: call_boxed::<P>,
    });
    self
  }

  pub(crate) fn platforms(&self) -> &[Entry] {
    &self.platforms
  }

  pub(crate) fn find(&self, name: &str) -> Option<&Entry> {
    self
      .platforms
      .iter()
      .find(|e| e.info.id == name || e.info.aliases.contains(&name))
  }

  pub(crate) fn command(&self) -> clap::Command {
    let mut cmd = GlobalArgs::augment_args(clap::Command::new(self.name))
      .version(self.version)
      .about(self.about)
      .long_about(None)
      .subcommand_required(true)
      .arg_required_else_help(true)
      .after_help("Guides: `atp guide` lists topics (start, login, analysis ...) and platforms.")
      .subcommand(
        clap::Command::new("platforms").about("List platforms and what each one supports"),
      )
      .subcommand(
        clap::Command::new("guide")
          .about(
            "Read the built-in documentation: `atp guide`, `atp guide analysis`, `atp guide bili`",
          )
          .visible_alias("docs")
          .arg(clap::Arg::new("topic").help("A topic or a platform; omit to list them")),
      )
      .subcommand(
        clap::Command::new("mcp")
          .about("Serve every platform to AI agents over MCP (stdio)")
          .arg(
            clap::Arg::new("read-only")
              .long("read-only")
              .action(clap::ArgAction::SetTrue)
              .help("Offer only commands that read; no likes, posts, messages or downloads"),
          ),
      );
    for e in &self.platforms {
      cmd = cmd.subcommand((e.command)());
    }
    cmd
  }

  pub fn run(self) -> ExitCode {
    restore_sigpipe();
    let mut args: Vec<OsString> = std::env::args_os().collect();
    let invoked = args
      .first()
      .and_then(|a| Path::new(a).file_stem())
      .and_then(|s| s.to_str())
      .map(str::to_owned);
    if let Some(entry) = invoked.as_deref().and_then(|n| self.find(n)) {
      args.insert(1, entry.info.id.into());
    }
    let matches = match self.command().try_get_matches_from(args) {
      Ok(m) => m,
      Err(e) => e.exit(),
    };
    let global = match GlobalArgs::from_arg_matches(&matches) {
      Ok(g) => g,
      Err(e) => e.exit(),
    };
    init_logging(global.verbose);

    let Some((name, sub)) = matches.subcommand() else {
      return ExitCode::FAILURE;
    };
    if name == "platforms" {
      self.print_platforms(global.output_format());
      return ExitCode::SUCCESS;
    }
    if name == "guide" {
      return self.print_guide(sub.get_one::<String>("topic").map(String::as_str));
    }
    let Some(runtime) = runtime() else {
      return ExitCode::FAILURE;
    };
    if name == "mcp" {
      return runtime.block_on(crate::mcp::serve(&self, sub.get_flag("read-only")));
    }
    let Some(entry) = self.find(name) else {
      return ExitCode::FAILURE;
    };
    runtime.block_on((entry.run)(global, sub.clone()))
  }

  fn print_guide(&self, topic: Option<&str>) -> ExitCode {
    let Some(topic) = topic else {
      let platforms: Vec<_> = self
        .platforms
        .iter()
        .map(|e| (e.info.id, e.info.name))
        .collect();
      guide::print(&guide::index(&platforms));
      return ExitCode::SUCCESS;
    };
    if let Some(t) = guide::TOPICS.iter().find(|t| t.name == topic) {
      guide::print(t.body);
    } else if let Some(e) = self.find(topic) {
      guide::print(&guide::platform(&e.info, &(e.command)()));
    } else {
      eprintln!("error: no guide topic `{topic}`; `atp guide` lists them");
      return ExitCode::from(2);
    }
    ExitCode::SUCCESS
  }

  /// Every platform with its capabilities and own commands.
  pub(crate) fn platforms_value(&self) -> serde_json::Value {
    let list: Vec<_> = self
      .platforms
      .iter()
      .map(|e| {
        json!({
          "id": e.info.id,
          "name": e.info.name,
          "aliases": e.info.aliases,
          "home": e.info.home,
          "capabilities": e.info.caps.iter().map(|c| c.name()).collect::<Vec<_>>(),
          "extra_commands": (e.extras)(),
        })
      })
      .collect();
    json!({ "platforms": list })
  }

  fn print_platforms(&self, format: Format) {
    if format != Format::Table {
      output::emit(format, None, &Data::Value(self.platforms_value()));
      return;
    }
    let mut t = Table::new();
    t.load_style(presets::NOTHING)
      .set_content_arrangement(ContentArrangement::Dynamic);
    let mut header = vec![Cell::new("")];
    header.extend(
      self
        .platforms
        .iter()
        .map(|e| Cell::new(e.info.id).add_attribute(Attribute::Bold)),
    );
    t.set_header(header);
    for cap in Cap::ALL {
      let mut row = vec![Cell::new(cap.name())];
      row.extend(self.platforms.iter().map(|e| {
        Cell::new(if e.info.supports(*cap) { "✓" } else { "·" })
          .set_alignment(CellAlignment::Center)
      }));
      t.add_row(row);
    }
    println!("{t}");
    for e in &self.platforms {
      let extras = (e.extras)();
      if !extras.is_empty() {
        println!(
          "\n{} ({}) only: {}",
          e.info.name,
          e.info.id,
          extras.join(", ")
        );
      }
    }
  }
}

fn runtime() -> Option<tokio::runtime::Runtime> {
  match tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()
  {
    Ok(rt) => Some(rt),
    Err(e) => {
      eprintln!("error: cannot start the async runtime: {e}");
      None
    }
  }
}

fn init_logging(verbose: bool) {
  let filter = tracing_subscriber::EnvFilter::try_from_env("ATP_LOG").unwrap_or_else(|_| {
    tracing_subscriber::EnvFilter::new(if verbose { "warn,atp=debug" } else { "warn" })
  });
  let _ = tracing_subscriber::fmt()
    .with_env_filter(filter)
    .with_writer(std::io::stderr)
    .without_time()
    .try_init();
}

/// Rust ignores SIGPIPE, so printing into a closed pipe (`atp ... | head`)
/// would panic; restore the default so the process just ends, like other tools.
fn restore_sigpipe() {
  #[cfg(unix)]
  // SAFETY: called once at startup, before any threads exist.
  unsafe {
    libc::signal(libc::SIGPIPE, libc::SIG_DFL);
  }
}

//! A platform's page, `atp guide <platform>`: its commands and option
//! values from clap, then its `GUIDE.md` with the login summary under
//! `## Logging in`.

use clap::Subcommand;

use super::{login, sections};
use crate::cli::CommonCommand;
use crate::platform::PlatformInfo;

/// Options of shared commands whose values each platform chooses.
const CHOICES: &[(&str, &str, &str)] = &[
  ("search", "sort", "--sort"),
  ("search", "filter", "--filter"),
  ("hot", "category", "--category"),
  ("feed", "kind", "--type"),
  ("comments", "sort", "--sort"),
  ("notifications", "kind", "--type"),
  ("following", "sort", "--sort"),
  ("follow-group", "sort", "--sort"),
];

fn about(cmd: &clap::Command) -> String {
  cmd.get_about().map(|a| a.to_string()).unwrap_or_default()
}

/// A platform's page: generated command list plus its own notes.
pub fn platform(info: &PlatformInfo, cmd: &clap::Command) -> String {
  let mut out = format!("# {} (`{}`)\n\n{}. ", info.name, info.id, info.about);
  let aliases: Vec<String> = info.aliases.iter().map(|a| format!("`{a}`")).collect();
  if !aliases.is_empty() {
    out.push_str(&format!("Also answers to {}. ", aliases.join(", ")));
  }
  out.push_str(&format!(
    "Home: {}\n\nOne command in detail: `atp guide {id} <command>` (e.g. `atp guide {id} \
     {example}`); logging in: `atp guide login {id}`.\n\n",
    info.home,
    id = info.id,
    example = first_command(cmd),
  ));
  out.push_str(&commands(info, cmd));
  out.push_str(&option_values(cmd));
  out.push('\n');
  out.push_str(&sections::with_login(
    info.guide.trim(),
    &login::summary(info),
  ));
  out.push('\n');
  out
}

fn visible(cmd: &clap::Command) -> Vec<&clap::Command> {
  let mut list: Vec<&clap::Command> = cmd
    .get_subcommands()
    .filter(|c| !c.is_hide_set() && c.get_name() != "help")
    .collect();
  // `--help` order (adjusting a subcommand moves it to the end of the list).
  list.sort_by_key(|c| c.get_display_order());
  list
}

fn first_command(cmd: &clap::Command) -> String {
  visible(cmd)
    .into_iter()
    .map(clap::Command::get_name)
    .find(|n| !["login", "logout", "status", "whoami"].contains(n))
    .unwrap_or("login")
    .to_owned()
}

/// The visible commands, shared ones first, in `--help` order; groups
/// list their subcommands.
fn commands(info: &PlatformInfo, cmd: &clap::Command) -> String {
  let (shared, own): (Vec<&clap::Command>, Vec<&clap::Command>) = visible(cmd)
    .into_iter()
    .partition(|c| CommonCommand::has_subcommand(c.get_name()));
  let mut out = String::from("## Shared commands\n\n");
  for c in &shared {
    out.push_str(&line(c));
  }
  if !own.is_empty() {
    out.push_str(&format!("\n## Only on {}\n\n", info.name));
    for c in &own {
      out.push_str(&line(c));
    }
  }
  out
}

fn line(c: &clap::Command) -> String {
  let subs: Vec<String> = visible(c)
    .iter()
    .map(|s| format!("`{}`", s.get_name()))
    .collect();
  if subs.is_empty() {
    format!("- `{}`: {}\n", c.get_name(), about(c))
  } else {
    format!("- `{}` ({}): {}\n", c.get_name(), subs.join(", "), about(c))
  }
}

/// The values the platform's shared options take (`search --sort` ...).
fn option_values(cmd: &clap::Command) -> String {
  let mut values = String::new();
  for (sub, arg, flag) in CHOICES {
    let Some(c) = cmd.find_subcommand(sub).filter(|c| !c.is_hide_set()) else {
      continue;
    };
    let Some(a) = c
      .get_arguments()
      .find(|a| a.get_id() == *arg && !a.is_hide_set())
    else {
      continue;
    };
    let v: Vec<String> = a
      .get_possible_values()
      .iter()
      .map(|p| format!("`{}`", p.get_name()))
      .collect();
    if !v.is_empty() {
      values.push_str(&format!("- `{sub} {flag}`: {}\n", v.join(", ")));
    }
  }
  if values.is_empty() {
    values
  } else {
    format!("\n## Option values\n\n{values}")
  }
}

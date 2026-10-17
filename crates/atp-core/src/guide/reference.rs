//! The generated part of a command page: what clap knows about a command
//! (usage, arguments, options with their values and defaults, aliases,
//! subcommands, examples), as Markdown.

use clap::{Arg, ArgAction, Command};

/// Usage, arguments, options, subcommands and examples of `cmd`, a
/// subcommand of a built `atp` command.
pub fn reference(cmd: &mut Command) -> String {
  let usage = cmd.render_usage().to_string();
  let usage = usage.trim().trim_start_matches("Usage:").trim();
  let mut out = String::new();
  if let Some(about) = cmd.get_long_about().or(cmd.get_about()) {
    out.push_str(&format!("{}\n\n", about.to_string().trim()));
  }
  out.push_str(&format!("## Usage\n\n    {usage}\n"));
  let aliases: Vec<String> = cmd
    .get_visible_aliases()
    .map(|a| format!("`{a}`"))
    .collect();
  if !aliases.is_empty() {
    out.push_str(&format!("\nAlso: {}.\n", aliases.join(", ")));
  }
  out.push_str(&arguments(cmd));
  out.push_str(&options(cmd));
  out.push_str(&subcommands(cmd));
  out.push_str(&examples(cmd));
  out
}

fn shown(a: &Arg) -> bool {
  !a.is_hide_set() && !a.is_global_set() && !["help", "version"].contains(&a.get_id().as_str())
}

fn arguments(cmd: &Command) -> String {
  let lines: Vec<String> = cmd
    .get_positionals()
    .filter(|a| shown(a))
    .map(|a| {
      let name = format!("<{}>{}", value_name(a), dots(a));
      let name = if a.is_required_set() {
        name
      } else {
        format!("[{name}]")
      };
      format!("- `{name}`: {}\n", describe(a))
    })
    .collect();
  section("Arguments", &lines)
}

/// Global options a platform adds itself (`--coord` of map platforms),
/// listed with the command's own.
fn platform_global(a: &Arg) -> bool {
  a.is_global_set() && !a.is_hide_set() && a.get_id() == crate::cli::COORD
}

fn options(cmd: &Command) -> String {
  let mut args: Vec<&Arg> = cmd
    .get_arguments()
    .filter(|a| (shown(a) || platform_global(a)) && !a.is_positional())
    .collect();
  // `--help` order: options a platform adjusted keep their place.
  args.sort_by_key(|a| a.get_display_order());
  let lines: Vec<String> = args
    .iter()
    .map(|a| format!("- `{}`: {}\n", flag(a), describe(a)))
    .collect();
  let mut out = section("Options", &lines);
  out.push_str(
    "\nGlobal options (`-f/--format`, `--json`, `--raw`, `--proxy`, `--interval`, \
     `--no-verify` ...) work with every command: `atp guide config`.\n",
  );
  out
}

fn subcommands(cmd: &Command) -> String {
  let lines: Vec<String> = cmd
    .get_subcommands()
    .filter(|c| !c.is_hide_set() && c.get_name() != "help")
    .map(|c| {
      let about = c.get_about().map(|a| a.to_string()).unwrap_or_default();
      format!("- `{}`: {about}\n", c.get_name())
    })
    .collect();
  section("Subcommands", &lines)
}

/// The text after the help (`after_long_help`), examples as code.
fn examples(cmd: &Command) -> String {
  let Some(text) = cmd.get_after_long_help().or(cmd.get_after_help()) else {
    return String::new();
  };
  let text = text.to_string();
  let title = if text.contains("Examples:") {
    "Examples"
  } else {
    "More"
  };
  let mut out = format!("\n## {title}\n\n");
  let mut code = false;
  for line in text.lines() {
    if line.trim_end() == "Examples:" {
      continue;
    }
    let indented = line.starts_with("  ");
    if code && !indented {
      out.push('\n');
    }
    if indented {
      if !code && !out.ends_with("\n\n") {
        out.push('\n');
      }
      out.push_str(&format!("    {}\n", line.trim_start()));
    } else if !line.trim().is_empty() {
      out.push_str(&format!("{}\n", line.trim()));
    }
    code = indented;
  }
  out
}

fn section(title: &str, lines: &[String]) -> String {
  if lines.is_empty() {
    String::new()
  } else {
    format!("\n## {title}\n\n{}", lines.concat())
  }
}

fn value_name(a: &Arg) -> String {
  a.get_value_names()
    .map(|v| v.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(" "))
    .unwrap_or_else(|| a.get_id().as_str().to_uppercase())
}

fn dots(a: &Arg) -> &'static str {
  if a.get_num_args().is_some_and(|r| r.max_values() > 1) {
    "..."
  } else {
    ""
  }
}

/// `-s, --sort <SORT>`.
fn flag(a: &Arg) -> String {
  let mut names = Vec::new();
  if let Some(s) = a.get_short() {
    names.push(format!("-{s}"));
  }
  if let Some(l) = a.get_long() {
    names.push(format!("--{l}"));
  }
  let mut out = names.join(", ");
  if a.get_action().takes_values() {
    let name = format!("<{}>", value_name(a));
    if a.get_num_args().is_some_and(|r| r.min_values() == 0) {
      out.push_str(&format!(" [{name}]"));
    } else {
      out.push_str(&format!(" {name}{}", dots(a)));
    }
  }
  out
}

/// The help text, then aliases, values, default and environment.
fn describe(a: &Arg) -> String {
  let help = a
    .get_long_help()
    .or(a.get_help())
    .map(|h| {
      h.to_string()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
    })
    .unwrap_or_default();
  let mut parts = vec![help.trim_end_matches('.').to_owned()];
  if a.is_required_set() && !a.is_positional() {
    parts.push("Required".into());
  }
  if matches!(a.get_action(), ArgAction::Append) {
    parts.push("Repeatable".into());
  }
  if let Some(aliases) = a.get_visible_aliases().filter(|v| !v.is_empty()) {
    let list: Vec<String> = aliases.iter().map(|x| format!("`--{x}`")).collect();
    parts.push(format!("Also {}", list.join(", ")));
  }
  if a.get_action().takes_values() {
    parts.extend(values(a));
  }
  if let Some(env) = a.get_env() {
    parts.push(format!("Environment: `{}`", env.to_string_lossy()));
  }
  parts.retain(|p| !p.is_empty());
  let mut out = parts.join(". ");
  if !out.is_empty() {
    out.push('.');
  }
  out
}

fn values(a: &Arg) -> Vec<String> {
  let mut out = Vec::new();
  let values: Vec<String> = a
    .get_possible_values()
    .iter()
    .filter(|v| !v.is_hide_set())
    .map(|v| format!("`{}`", v.get_name()))
    .collect();
  if !values.is_empty() {
    out.push(format!("Values: {}", values.join(", ")));
  }
  let defaults: Vec<String> = a
    .get_default_values()
    .iter()
    .map(|d| format!("`{}`", d.to_string_lossy()))
    .collect();
  if !defaults.is_empty() && !a.is_hide_default_value_set() {
    out.push(format!("Default: {}", defaults.join(", ")));
  }
  out
}

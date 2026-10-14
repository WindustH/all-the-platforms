//! Which page a request names: `atp guide WORDS...`, the MCP `help` tool
//! and `atp://guide/...` resources all come here.

use crate::app::{App, Entry};
use crate::platform::PlatformInfo;

use super::{command, index, login, platform, topic};

/// The page for `words` (`[]`, `[topic]`, `[login]`, `[login, platform]`,
/// `[platform]`, `[platform, command, sub...]`; words may hold spaces), or
/// why there is none, with suggestions.
pub fn page(app: &App, words: &[&str]) -> Result<String, String> {
  let words: Vec<&str> = words.iter().flat_map(|w| w.split_whitespace()).collect();
  let infos: Vec<PlatformInfo> = app.platforms().iter().map(|e| e.info).collect();
  let Some((first, rest)) = words.split_first() else {
    return Ok(index(&infos));
  };
  if *first == "login" {
    return match rest {
      [] => Ok(login::overview(&infos)),
      [p] => app
        .find(p)
        .map(|e| login::page(&e.info))
        .ok_or_else(|| unknown_platform(app, p)),
      _ => Err("`login` takes one platform: `atp guide login xhs`".into()),
    };
  }
  if let Some(t) = topic(first) {
    return match rest {
      [] => Ok(t.body.to_owned()),
      _ => Err(format!(
        "the topic `{first}` has no parts; read `atp guide {first}`"
      )),
    };
  }
  let entry = app.find(first).ok_or_else(|| unknown(app, first))?;
  if rest.is_empty() {
    return Ok(platform(&entry.info, &(entry.command)()));
  }
  command_page(entry, rest)
}

fn command_page(entry: &Entry, words: &[&str]) -> Result<String, String> {
  let info = entry.info;
  let mut root = clap::Command::new("atp").subcommand((entry.command)());
  root.build();
  let path = resolve(&root, &info, words)?;
  let missing = || format!("{} lost its command `{}`", info.id, path.join(" "));
  let mut cmd = root.find_subcommand_mut(info.id).ok_or_else(missing)?;
  for name in &path {
    cmd = cmd.find_subcommand_mut(name).ok_or_else(missing)?;
  }
  Ok(command::page(&info, cmd, &path))
}

/// The canonical names of the commands `words` name (aliases resolved).
fn resolve(
  root: &clap::Command,
  info: &PlatformInfo,
  words: &[&str],
) -> Result<Vec<String>, String> {
  let mut cmd = root
    .find_subcommand(info.id)
    .ok_or_else(|| format!("no platform `{}`", info.id))?;
  let mut path: Vec<String> = Vec::new();
  for word in words {
    let Some(sub) = cmd.find_subcommand(word).filter(|c| c.get_name() != "help") else {
      return Err(no_command(info, cmd, &path, word));
    };
    if sub.is_hide_set() {
      return Err(format!(
        "{} does not support `{}`; `atp guide {}` lists what it does",
        info.name,
        sub.get_name(),
        info.id
      ));
    }
    path.push(sub.get_name().to_owned());
    cmd = sub;
  }
  Ok(path)
}

fn no_command(info: &PlatformInfo, cmd: &clap::Command, path: &[String], word: &str) -> String {
  let names: Vec<&str> = cmd
    .get_subcommands()
    .filter(|c| !c.is_hide_set() && c.get_name() != "help")
    .flat_map(|c| std::iter::once(c.get_name()).chain(c.get_visible_aliases()))
    .collect();
  let at = if path.is_empty() {
    info.name.to_owned()
  } else {
    format!("`atp {} {}`", info.id, path.join(" "))
  };
  if names.is_empty() {
    return format!("{at} has no subcommands");
  }
  let mut out = format!("{at} has no command `{word}`.");
  out.push_str(&did_you_mean(word, &names));
  if names.len() <= 12 {
    out.push_str(&format!(" Its commands: {}.", names.join(", ")));
  } else {
    out.push_str(&format!(" `atp guide {}` lists them.", info.id));
  }
  out
}

fn unknown(app: &App, word: &str) -> String {
  let mut names: Vec<&str> = super::TOPICS.iter().map(|t| t.name).collect();
  names.extend(platform_names(app));
  format!(
    "no guide topic or platform `{word}`.{} `atp guide` lists them.",
    did_you_mean(word, &names)
  )
}

fn unknown_platform(app: &App, word: &str) -> String {
  format!(
    "no platform `{word}`.{} `atp guide` lists the platforms.",
    did_you_mean(word, &platform_names(app))
  )
}

fn platform_names(app: &App) -> Vec<&'static str> {
  app
    .platforms()
    .iter()
    .flat_map(|e| std::iter::once(e.info.id).chain(e.info.aliases.iter().copied()))
    .collect()
}

/// ` Did you mean `x`?` for names close to `word`, else nothing.
pub(crate) fn did_you_mean(word: &str, names: &[&str]) -> String {
  let word = word.to_lowercase();
  let mut close: Vec<(usize, &str)> = names
    .iter()
    .map(|n| (distance(&word, n), *n))
    .filter(|(d, n)| {
      *d <= (word.chars().count() / 3).max(1) || (word.len() > 1 && n.starts_with(&word))
    })
    .collect();
  close.sort();
  close.dedup_by(|a, b| a.1 == b.1);
  let close: Vec<String> = close
    .iter()
    .take(3)
    .map(|(_, n)| format!("`{n}`"))
    .collect();
  if close.is_empty() {
    String::new()
  } else {
    format!(" Did you mean {}?", crate::platform::either(&close))
  }
}

/// Edit distance between two words, a swap of neighbours counting as one
/// edit (`blii` is one away from `bili`).
fn distance(a: &str, b: &str) -> usize {
  let a: Vec<char> = a.chars().collect();
  let b: Vec<char> = b.chars().collect();
  let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
  for (i, row) in d.iter_mut().enumerate() {
    row[0] = i;
  }
  for (j, cell) in d[0].iter_mut().enumerate() {
    *cell = j;
  }
  for i in 1..=a.len() {
    for j in 1..=b.len() {
      let cost = usize::from(a[i - 1] != b[j - 1]);
      let mut best = (d[i - 1][j] + 1)
        .min(d[i][j - 1] + 1)
        .min(d[i - 1][j - 1] + cost);
      if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
        best = best.min(d[i - 2][j - 2] + 1);
      }
      d[i][j] = best;
    }
  }
  d[a.len()][b.len()]
}

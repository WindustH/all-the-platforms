//! A command page, `atp guide <platform> <command> [<sub>...]`: the
//! generated reference, what the command means on every platform, and the
//! platform's own notes on it.

use clap::Subcommand;

use super::{reference, sections};
use crate::cli::CommonCommand;
use crate::platform::PlatformInfo;

/// What shared commands mean on every platform, one file per group.
const SHARED: &[(&[&str], &str)] = &[
  (&["search"], include_str!("../../guide/commands/search.md")),
  (
    &["hot", "feed"],
    include_str!("../../guide/commands/hot.md"),
  ),
  (&["read"], include_str!("../../guide/commands/read.md")),
  (
    &["comments", "replies"],
    include_str!("../../guide/commands/comments.md"),
  ),
  (
    &["likers", "reposts"],
    include_str!("../../guide/commands/likers.md"),
  ),
  (
    &["user", "user-posts", "followers", "following"],
    include_str!("../../guide/commands/user.md"),
  ),
  (
    &["favorites", "collections"],
    include_str!("../../guide/commands/favorites.md"),
  ),
  (&["likes"], include_str!("../../guide/commands/likes.md")),
  (
    &["history"],
    include_str!("../../guide/commands/history.md"),
  ),
  (
    &["notifications", "unread"],
    include_str!("../../guide/commands/notifications.md"),
  ),
  (
    &["download"],
    include_str!("../../guide/commands/download.md"),
  ),
  (
    &[
      "like",
      "unlike",
      "favorite",
      "unfavorite",
      "follow",
      "unfollow",
    ],
    include_str!("../../guide/commands/like.md"),
  ),
  (
    &["block", "unblock", "blocked", "remove-follower"],
    include_str!("../../guide/commands/block.md"),
  ),
  (
    &[
      "follow-groups",
      "follow-group",
      "follow-group-create",
      "follow-group-rename",
      "follow-group-delete",
      "follow-group-add",
      "follow-group-remove",
    ],
    include_str!("../../guide/commands/follow-groups.md"),
  ),
  (
    &[
      "folder-create",
      "folder-edit",
      "folder-delete",
      "folder-add",
      "folder-remove",
      "folder-move",
    ],
    include_str!("../../guide/commands/folders.md"),
  ),
  (
    &["comment", "delete-comment"],
    include_str!("../../guide/commands/comment.md"),
  ),
  (&["post"], include_str!("../../guide/commands/post.md")),
  (&["delete"], include_str!("../../guide/commands/delete.md")),
  (
    &["chats", "members"],
    include_str!("../../guide/commands/chats.md"),
  ),
  (
    &["messages"],
    include_str!("../../guide/commands/messages.md"),
  ),
  (&["send"], include_str!("../../guide/commands/send.md")),
  (&["react"], include_str!("../../guide/commands/react.md")),
  (&["edit"], include_str!("../../guide/commands/edit.md")),
  (&["listen"], include_str!("../../guide/commands/listen.md")),
  (
    &["insights"],
    include_str!("../../guide/commands/insights.md"),
  ),
  (&["login"], include_str!("../../guide/commands/login.md")),
  (
    &["logout", "status", "whoami"],
    include_str!("../../guide/commands/status.md"),
  ),
];

/// The shared meaning of `name`, when it is a shared command with one.
fn shared(name: &str) -> Option<&'static str> {
  if !CommonCommand::has_subcommand(name) {
    return None;
  }
  SHARED
    .iter()
    .find(|(names, _)| names.contains(&name))
    .map(|(_, body)| body.trim())
}

/// The page of `cmd`, the command at `path` under the platform `info`, in
/// a built `atp` command.
pub fn page(info: &PlatformInfo, cmd: &mut clap::Command, path: &[String]) -> String {
  let words = path.join(" ");
  let mut out = format!("# atp {} {words}\n\n", info.id);
  out.push_str(&reference::reference(cmd));
  if let [name] = path
    && let Some(body) = shared(name)
  {
    out.push_str(&format!("\n## On every platform\n\n{body}\n"));
  }
  let notes = notes(info, path);
  if !notes.is_empty() {
    out.push_str(&format!("\n## On {}\n\n{notes}", info.name));
  }
  out.push_str(&format!(
    "\nMore: `atp guide {}` covers all of {}",
    info.id, info.name
  ));
  if path[0].starts_with("login") {
    out.push_str(&format!(
      "; `atp guide login {}` is about logging in",
      info.id
    ));
  }
  out.push_str(".\n");
  out
}

/// The platform's notes on the command: its `## Logging in` section for
/// `login` and `login-*` commands, else the `### ` sections naming it (or
/// the group it is part of) under `## Commands`.
fn notes(info: &PlatformInfo, path: &[String]) -> String {
  if path[0] == "login" || path[0].starts_with("login-") {
    return sections::section(info.guide, sections::LOGIN)
      .map(|s| format!("{s}\n"))
      .unwrap_or_default();
  }
  let words: Vec<&str> = path.iter().map(String::as_str).collect();
  let found = sections::notes(info.guide, &words);
  let own = path.join(" ");
  let mut out = String::new();
  for (heading, body) in &found {
    if found.len() > 1 || *heading != own {
      out.push_str(&format!("### {heading}\n\n"));
    }
    out.push_str(&format!("{body}\n\n"));
  }
  out.truncate(out.trim_end().len());
  if !out.is_empty() {
    out.push('\n');
  }
  out
}

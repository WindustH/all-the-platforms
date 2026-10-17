//! `atp guide`: usage documentation built into the binary.
//!
//! General topics are Markdown files under `crates/atp-core/guide/`; what
//! a shared command means on every platform is under
//! `crates/atp-core/guide/commands/`. Pages about a platform are generated
//! from what the binary knows (its clap commands, option values, login
//! ways) followed by the platform crate's `GUIDE.md`
//! (`PlatformInfo::guide`), whole or one section at a time, so they cannot
//! drift from the real command set. The same pages serve `atp guide` and
//! the MCP server ([`page`]). Terminals get light styling; pipes get the
//! Markdown as is.

mod command;
mod login;
mod lookup;
mod platform;
mod reference;
mod render;
mod sections;

pub(crate) use self::lookup::did_you_mean;
pub use self::lookup::page;
pub use self::platform::platform;
pub use self::render::print;

use crate::platform::{PlatformInfo, PlatformKind};

pub struct Topic {
  pub name: &'static str,
  pub summary: &'static str,
  pub body: &'static str,
}

pub const TOPICS: &[Topic] = &[
  Topic {
    name: "start",
    summary: "What ATP does, first commands, where to get help",
    body: include_str!("../../guide/start.md"),
  },
  Topic {
    name: "login",
    summary: "QR codes, browser sessions, cookie strings, tokens; every platform's login ways",
    body: include_str!("../../guide/login.md"),
  },
  Topic {
    name: "refs",
    summary: "Ids, links, #N, --limit, --cursor, --since / --until",
    body: include_str!("../../guide/refs.md"),
  },
  Topic {
    name: "output",
    summary: "table, json, yaml, jsonl, csv; the envelope, error codes and data shapes",
    body: include_str!("../../guide/output.md"),
  },
  Topic {
    name: "analysis",
    summary: "Creator insights, likers, reposts, full comment threads, exports, snapshots",
    body: include_str!("../../guide/analysis.md"),
  },
  Topic {
    name: "maps",
    summary: "Map platforms: places, routes, distances; points and --coord (wgs84, gcj02, bd09)",
    body: include_str!("../../guide/maps.md"),
  },
  Topic {
    name: "download",
    summary: "Images, videos, audio only, speech-recognition segments",
    body: include_str!("../../guide/download.md"),
  },
  Topic {
    name: "interact",
    summary: "Likes, favorites, comments, follows, blocks, follow groups, folders, posting",
    body: include_str!("../../guide/interact.md"),
  },
  Topic {
    name: "mcp",
    summary: "Serving every platform to AI agents over MCP (`atp mcp`)",
    body: include_str!("../../guide/mcp.md"),
  },
  Topic {
    name: "config",
    summary: "Global options, environment variables, files, exit codes",
    body: include_str!("../../guide/config.md"),
  },
];

pub(crate) fn topic(name: &str) -> Option<&'static Topic> {
  TOPICS.iter().find(|t| t.name == name)
}

/// The index of `atp guide`: how to ask, the topics, the platforms by kind.
pub fn index(platforms: &[PlatformInfo]) -> String {
  let mut out = String::from(INDEX_HEAD);
  out.push_str("\n## Topics\n\n");
  for t in TOPICS {
    out.push_str(&format!("- `{}`: {}\n", t.name, t.summary));
  }
  out.push_str("\n## Platforms\n");
  for kind in PlatformKind::ALL {
    let members: Vec<&PlatformInfo> = platforms.iter().filter(|p| p.kind == *kind).collect();
    if members.is_empty() {
      continue;
    }
    out.push_str(&format!("\n### {}\n\n", kind.title()));
    for p in members {
      out.push_str(&format!("- `{}`: {}\n", p.id, p.about));
    }
  }
  out
}

const INDEX_HEAD: &str = "\
# ATP guide

ATP carries its own documentation. Read a page with:

    atp guide <topic>                           # e.g. atp guide refs
    atp guide <platform>                        # e.g. atp guide bili
    atp guide <platform> <command> [<sub>...]   # e.g. atp guide bili search
    atp guide login [<platform>]                # e.g. atp guide login xhs

A command page joins the command's usage, options and examples, what it
means on every platform, and the platform's own notes:
`atp guide ncm playlist create`. Pages are Markdown, lightly styled on a
terminal. AI agents get the same pages from `atp mcp`: the `help` tool and
the `atp://guide/...` resources (`atp guide mcp`).
";

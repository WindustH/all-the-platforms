//! The tools: `platforms`, `help`, `read` (commands that only read) and
//! `run` (everything else, left out with `--read-only`).

use std::time::Duration;

use clap::FromArgMatches;
use serde_json::{Value, json};

use crate::app::App;
use crate::cli::GlobalArgs;
use crate::error::{Error, Result};
use crate::guide;
use crate::json::ValueExt;
use crate::model::{Data, Page};
use crate::output;

/// Shared commands that only read.
const READS: &[&str] = &[
  "search",
  "hot",
  "feed",
  "read",
  "comments",
  "replies",
  "likers",
  "reposts",
  "insights",
  "user",
  "user-posts",
  "followers",
  "following",
  "collections",
  "favorites",
  "likes",
  "history",
  "notifications",
  "unread",
  "chats",
  "messages",
  "members",
  "whoami",
  "status",
];

/// Seconds a command may run: `listen` and other live commands return
/// the events seen when the time is up.
const WAIT_DEFAULT: u64 = 300;
const WAIT_LIVE: u64 = 30;
const WAIT_MAX: u64 = 1800;

type Reply = std::result::Result<Value, (i64, String)>;

pub struct Tools<'a> {
  app: &'a App,
  read_only: bool,
}

impl<'a> Tools<'a> {
  pub fn new(app: &'a App, read_only: bool) -> Self {
    Self { app, read_only }
  }

  pub fn list(&self) -> Vec<Value> {
    let ids: Vec<&str> = self.app.platforms().iter().map(|e| e.info.id).collect();
    let mut tools = vec![platforms_tool(), help_tool(), read_tool(&ids)];
    if !self.read_only {
      tools.push(run_tool(&ids));
    }
    tools
  }

  pub async fn call(&self, params: &Value) -> Reply {
    let args = params.at("arguments");
    let text = match params.str("name").unwrap_or_default().as_str() {
      "platforms" => Ok(self.app.platforms_value().to_string()),
      "help" => self.help(args.str("topic").unwrap_or_default().trim()),
      "read" => self.command(args, true).await,
      "run" if !self.read_only => self.command(args, false).await,
      other => return Err((-32602, format!("unknown tool: {other}"))),
    };
    let (text, failed) = match text {
      Ok(text) => {
        let failed = text.starts_with("{\"ok\":false");
        (text, failed)
      }
      Err(message) => (message, true),
    };
    Ok(json!({"content": [{"type": "text", "text": text}], "isError": failed}))
  }

  /// A guide topic, a platform's guide or one command's help.
  fn help(&self, topic: &str) -> std::result::Result<String, String> {
    if topic.is_empty() {
      let platforms: Vec<_> = self
        .app
        .platforms()
        .iter()
        .map(|e| (e.info.id, e.info.name))
        .collect();
      return Ok(guide::index(&platforms));
    }
    if let Some(t) = guide::TOPICS.iter().find(|t| t.name == topic) {
      return Ok(t.body.to_owned());
    }
    let mut words = topic.split_whitespace();
    let entry = words
      .next()
      .and_then(|p| self.app.find(p))
      .ok_or_else(|| format!("no topic or platform `{topic}`; call help without a topic"))?;
    let Some(name) = words.next() else {
      return Ok(guide::platform(&entry.info, &(entry.command)()));
    };
    let mut root = self.app.command();
    root.build();
    root
      .find_subcommand_mut(entry.info.id)
      .and_then(|p| p.find_subcommand_mut(name))
      .map(|c| c.render_long_help().to_string())
      .ok_or_else(|| format!("{} has no command `{name}`", entry.info.id))
  }

  /// Run `platform command args...` and answer with its envelope.
  async fn command(&self, args: &Value, read: bool) -> std::result::Result<String, String> {
    let platform = args.str("platform").unwrap_or_default();
    let name = args.str("command").unwrap_or_default();
    let words: Vec<String> = args
      .list("args")
      .iter()
      .map(|w| w.as_str().map_or_else(|| w.to_string(), str::to_owned))
      .collect();
    check(&name, &words, read)?;
    let entry = self
      .app
      .find(&platform)
      .ok_or_else(|| format!("unknown platform `{platform}`; see the platforms tool"))?;
    let argv = ["atp", entry.info.id, name.as_str()]
      .into_iter()
      .map(str::to_owned)
      .chain(words);
    let matches = match self.app.command().try_get_matches_from(argv) {
      Ok(m) => m,
      Err(e) => return Err(e.render().to_string()),
    };
    let mut global = GlobalArgs::from_arg_matches(&matches).map_err(|e| e.to_string())?;
    global.no_verify = true;
    let sub = matches
      .subcommand_matches(entry.info.id)
      .cloned()
      .unwrap_or_default();
    let wait = args.u64("wait_secs").unwrap_or(if name == "listen" {
      WAIT_LIVE
    } else {
      WAIT_DEFAULT
    });
    let result = limited((entry.call)(global, sub), &name, wait.min(WAIT_MAX)).await;
    Ok(output::envelope(Some(entry.info.id), &result).to_string())
  }
}

/// Refuse what cannot work over MCP, and writes on the `read` tool.
fn check(name: &str, words: &[String], read: bool) -> std::result::Result<(), String> {
  if read && !READS.contains(&name) {
    return Err(format!(
      "`{name}` is not a read-only command; the read tool runs {}",
      READS.join(", ")
    ));
  }
  if words.iter().any(|w| w == "-") {
    return Err("`-` (standard input) is not available over MCP; pass the text itself".into());
  }
  let given = |flag: &str| words.iter().any(|w| w.starts_with(flag));
  if name == "login"
    && !["--cookie", "--token", "--browser"]
      .iter()
      .any(|f| given(f))
  {
    return Err(
      "a QR-code login needs a terminal: run `atp <platform> login` there, or pass \
       --cookie, --token or --browser"
        .into(),
    );
  }
  Ok(())
}

/// Run `call` for at most `secs`, collecting live events; when time is up
/// the events seen so far are the result.
async fn limited(
  call: impl std::future::Future<Output = Result<Data>>,
  name: &str,
  secs: u64,
) -> Result<Data> {
  output::capture();
  let result = tokio::time::timeout(Duration::from_secs(secs), call).await;
  let events = output::captured();
  match result {
    Ok(done) => done,
    Err(_) if !events.is_empty() || name == "listen" => Ok(Data::Posts(Page::new(events, None))),
    Err(_) => Err(Error::network(format!(
      "`{name}` gave no result within {secs} s; raise wait_secs, or it is a live command \
       that saw no events"
    ))),
  }
}

fn platforms_tool() -> Value {
  json!({
    "name": "platforms",
    "title": "List platforms",
    "description": "Every platform atp supports: id, name, aliases, capabilities and its own extra commands.",
    "inputSchema": {"type": "object", "properties": {}},
    "annotations": {"readOnlyHint": true, "openWorldHint": false},
  })
}

fn help_tool() -> Value {
  json!({
    "name": "help",
    "title": "Read the guide",
    "description": "Documentation: no topic lists the topics; a topic (start, login, refs, output, \
      analysis, download, interact, mcp, config); a platform (`bili`) for its commands, references and \
      limits; or a platform and command (`bili search`) for that command's options.",
    "inputSchema": {
      "type": "object",
      "properties": {"topic": {"type": "string", "description": "Topic, platform, or `platform command`"}},
    },
    "annotations": {"readOnlyHint": true, "openWorldHint": false},
  })
}

/// Properties shared by `read` and `run`.
fn command_schema(ids: &[&str], command: Value) -> Value {
  json!({
    "type": "object",
    "properties": {
      "platform": {"type": "string", "enum": ids, "description": "Platform id"},
      "command": command,
      "args": {
        "type": "array",
        "items": {"type": "string"},
        "description": "Further command-line words: references, text, options (`-n 50`, `--sort hot`, `--yes`). \
          `#3` is the third item of the last list.",
      },
      "wait_secs": {
        "type": "integer",
        "minimum": 1,
        "maximum": WAIT_MAX,
        "description": "Time limit; live commands (listen) return the events seen by then. \
          Default 30 for listen, 300 otherwise.",
      },
    },
    "required": ["platform", "command"],
  })
}

fn read_tool(ids: &[&str]) -> Value {
  let command =
    json!({"type": "string", "enum": READS, "description": "A command that only reads"});
  json!({
    "name": "read",
    "title": "Read from a platform",
    "description": "Run a command that only reads: search, hot lists, feeds, posts, comments, users, \
      followers, favorites, notifications, chats and messages, account status. \
      Returns the JSON envelope {ok, data | error}. `help` with `<platform> <command>` shows the options.",
    "inputSchema": command_schema(ids, command),
    "annotations": {"readOnlyHint": true, "openWorldHint": true},
  })
}

fn run_tool(ids: &[&str]) -> Value {
  let command =
    json!({"type": "string", "description": "Any command of the platform, shared or its own"});
  json!({
    "name": "run",
    "title": "Act on a platform",
    "description": "Run any command, including ones that change things: like, favorite, comment, follow, \
      post, delete, send, react, edit, download, login with --cookie / --token / --browser, logout, \
      listen, and each platform's own commands (see `platforms`). Irreversible commands need `--yes` in args. \
      Returns the JSON envelope {ok, data | error}.",
    "inputSchema": command_schema(ids, command),
    "annotations": {"readOnlyHint": false, "destructiveHint": true, "openWorldHint": true},
  })
}

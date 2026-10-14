//! `atp mcp`: every platform served to AI agents over the Model Context
//! Protocol — JSON-RPC 2.0, one message per line on stdin / stdout.
//!
//! Requests are handled one at a time on the program's single thread, so
//! commands keep their pacing and `#N` references carry from call to call.
//! Nothing else may write to stdout while serving: results come back as
//! envelopes and live events are collected (see [`tools`]).

mod resources;
mod tools;

use std::io::Write;
use std::process::ExitCode;

use serde_json::{Value, json};

use crate::app::App;
use crate::json::ValueExt;

/// Protocol revisions this server speaks, newest first.
const VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

const INSTRUCTIONS: &str = "ATP (all-the-platforms) reads and acts on social, video, chat, \
shopping, map, music and drive platforms with one command set. `platforms` lists them and what \
each supports. `help` reads the guide: a topic, `login` or `login <platform>` (how to log in, \
with steps, tokens and risks), a platform (`bili`), or one command (`bili search`, \
`ncm playlist create`) with its options, values, examples and the platform's notes; the same \
pages are resources under atp://guide/. `read` runs commands that only read (search, hot, \
read, comments, user, ...); `run` runs anything else (likes, comments, posts, messages, \
downloads, platform-only commands). Arguments are command-line words, e.g. platform `bili`, \
command `search`, args [\"rust\", \"-n\", \"5\"]. Results are JSON envelopes \
{ok, data | error{code, message, hint}}. `#3` in args means the third item of the last list. \
Logging in with a QR code needs a terminal (`atp <platform> login`); `run` takes \
`login --cookie`, `--token` or `--browser`.";

/// Serve until stdin closes.
pub async fn serve(app: &App, read_only: bool) -> ExitCode {
  let server = tools::Tools::new(app, read_only);
  while let Some(line) = read_line().await {
    if line.trim().is_empty() {
      continue;
    }
    let reply = match serde_json::from_str::<Value>(&line) {
      Ok(Value::Array(batch)) => {
        let mut replies = Vec::new();
        for message in batch {
          replies.extend(handle(&server, app, message).await);
        }
        (!replies.is_empty()).then_some(Value::Array(replies))
      }
      Ok(message) => handle(&server, app, message).await,
      Err(e) => Some(failure(Value::Null, -32700, &format!("parse error: {e}"))),
    };
    if let Some(reply) = reply {
      write(&reply);
    }
  }
  ExitCode::SUCCESS
}

/// The next line of stdin, or `None` at its end.
async fn read_line() -> Option<String> {
  tokio::task::spawn_blocking(|| {
    let mut line = String::new();
    match std::io::stdin().read_line(&mut line) {
      Ok(0) | Err(_) => None,
      Ok(_) => Some(line),
    }
  })
  .await
  .ok()
  .flatten()
}

fn write(message: &Value) {
  let mut out = std::io::stdout().lock();
  let _ = writeln!(out, "{message}");
  let _ = out.flush();
}

/// The reply to one message; notifications (no `id`) get none.
async fn handle(server: &tools::Tools<'_>, app: &App, message: Value) -> Option<Value> {
  let id = message.get("id").cloned()?;
  let params = message.at("params");
  let result = match message.str("method").unwrap_or_default().as_str() {
    "initialize" => Ok(initialize(app, params)),
    "ping" => Ok(json!({})),
    "tools/list" => Ok(json!({ "tools": server.list() })),
    "tools/call" => server.call(params).await,
    "resources/list" => Ok(resources::list(app)),
    "resources/templates/list" => Ok(resources::templates()),
    "resources/read" => resources::read(app, params),
    other => Err((-32601, format!("method not found: {other}"))),
  };
  Some(match result {
    Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
    Err((code, message)) => failure(id, code, &message),
  })
}

fn initialize(app: &App, params: &Value) -> Value {
  let asked = params.str("protocolVersion").unwrap_or_default();
  let version = VERSIONS
    .iter()
    .find(|v| **v == asked)
    .unwrap_or(&VERSIONS[0]);
  json!({
    "protocolVersion": version,
    "capabilities": {
      "tools": { "listChanged": false },
      "resources": { "subscribe": false, "listChanged": false },
    },
    "serverInfo": {
      "name": "atp",
      "title": "ATP (all-the-platforms)",
      "version": app.version,
    },
    "instructions": INSTRUCTIONS,
  })
}

fn failure(id: Value, code: i64, message: &str) -> Value {
  json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

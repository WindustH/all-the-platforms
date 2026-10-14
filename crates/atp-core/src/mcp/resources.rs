//! The guide as MCP resources: `atp://guide` (the index), one resource per
//! topic (`atp://guide/refs`, `atp://guide/login`) and per platform
//! (`atp://guide/bili`), and templates for one command
//! (`atp://guide/{platform}/{command}`, deeper paths for nested commands)
//! and one platform's login page (`atp://guide/login/{platform}`).
//! Every resource is the Markdown page `atp guide` prints.

use serde_json::{Value, json};

use crate::app::App;
use crate::guide;
use crate::json::ValueExt;

const ROOT: &str = "atp://guide";
const MARKDOWN: &str = "text/markdown";

/// JSON-RPC error code for an unknown resource (MCP spec).
const NOT_FOUND: i64 = -32002;

type Reply = std::result::Result<Value, (i64, String)>;

/// `resources/list`: the index, the topics and the platforms.
pub fn list(app: &App) -> Value {
  let mut resources = vec![resource(
    ROOT,
    "index",
    "ATP guide",
    "How to ask, the topics and the platforms",
  )];
  for t in guide::TOPICS {
    let summary = if t.name == "login" {
      format!("{}; with a table of every platform", t.summary)
    } else {
      t.summary.to_owned()
    };
    // The title is the topic's own heading: `# Getting started`.
    let title = t.body.lines().next().and_then(|l| l.strip_prefix("# "));
    resources.push(resource(
      &format!("{ROOT}/{}", t.name),
      t.name,
      title.unwrap_or(t.name),
      &summary,
    ));
  }
  for e in app.platforms() {
    let info = e.info;
    resources.push(resource(
      &format!("{ROOT}/{}", info.id),
      info.id,
      info.name,
      &format!("{}: commands, references, logging in, limits", info.about),
    ));
  }
  json!({ "resources": resources })
}

fn resource(uri: &str, name: &str, title: &str, description: &str) -> Value {
  json!({
    "uri": uri,
    "name": name,
    "title": title,
    "description": description,
    "mimeType": MARKDOWN,
  })
}

/// `resources/templates/list`: one command, one platform's login page.
pub fn templates() -> Value {
  json!({ "resourceTemplates": [
    {
      "uriTemplate": format!("{ROOT}/{{platform}}/{{command}}"),
      "name": "command",
      "title": "One command of a platform",
      "description": "Usage, options with values and defaults, examples, what the command means \
        on every platform and the platform's notes, e.g. atp://guide/bili/search. Nested \
        commands add path segments: atp://guide/ncm/playlist/create.",
      "mimeType": MARKDOWN,
    },
    {
      "uriTemplate": format!("{ROOT}/login/{{platform}}"),
      "name": "login",
      "title": "Logging in to one platform",
      "description": "Every way to log in with steps, where tokens come from, what each way \
        unlocks, session lifetime, risk control and environment variables.",
      "mimeType": MARKDOWN,
    },
  ]})
}

/// `resources/read`: the page a guide URI names.
pub fn read(app: &App, params: &Value) -> Reply {
  let uri = params.str("uri").unwrap_or_default();
  let Some(words) = words(&uri) else {
    return Err((
      NOT_FOUND,
      format!("resource not found: {uri}; guide URIs start with {ROOT}"),
    ));
  };
  let words: Vec<&str> = words.iter().map(String::as_str).collect();
  match guide::page(app, &words) {
    Ok(text) => Ok(json!({ "contents": [{"uri": uri, "mimeType": MARKDOWN, "text": text}] })),
    Err(message) => Err((NOT_FOUND, format!("resource not found: {uri}: {message}"))),
  }
}

/// The guide words of a URI: `atp://guide/ncm/playlist/create` →
/// [`ncm`, `playlist`, `create`]; `None` for other URIs.
fn words(uri: &str) -> Option<Vec<String>> {
  let rest = uri.strip_prefix(ROOT)?;
  if !(rest.is_empty() || rest.starts_with('/')) {
    return None;
  }
  let path = rest.split(['?', '#']).next().unwrap_or_default();
  Some(
    path
      .split('/')
      .filter(|s| !s.is_empty())
      .map(decode)
      .collect(),
  )
}

/// Percent-decoding of one path segment.
fn decode(segment: &str) -> String {
  let bytes = segment.as_bytes();
  let mut out = Vec::with_capacity(bytes.len());
  let mut i = 0;
  while i < bytes.len() {
    let hex = bytes
      .get(i + 1..i + 3)
      .and_then(|h| std::str::from_utf8(h).ok())
      .and_then(|h| u8::from_str_radix(h, 16).ok());
    match (bytes[i], hex) {
      (b'%', Some(b)) => {
        out.push(b);
        i += 3;
      }
      (b, _) => {
        out.push(b);
        i += 1;
      }
    }
  }
  String::from_utf8_lossy(&out).into_owned()
}

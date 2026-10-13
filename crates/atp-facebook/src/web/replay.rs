//! Replays of the GraphQL operations a page ran, with only the cursor and
//! count changed, and the streamed answer stitched like a page's payloads.

use atp_core::{Ctx, Error, Result, Value, ValueExt, json};

use super::Page;
use crate::refs::HOME;
use crate::relay::{self, COUNT_KEYS, CURSOR_KEYS, Preload, Tokens};

/// Replay with the cursor moved to `cursor` and the count set to `count`.
pub async fn advance(
  ctx: &Ctx,
  page: &Page,
  pre: &Preload,
  cursor: &str,
  count: usize,
) -> Result<Value> {
  let mut vars = serde_json::Map::new();
  for key in CURSOR_KEYS {
    if pre.variables.contains_key(key) {
      vars.insert(key.into(), json!(cursor));
    }
  }
  if vars.is_empty() {
    return Err(Error::unsupported("paging this list"));
  }
  if count > 0 {
    for key in COUNT_KEYS {
      if pre.variables.contains_key(key) {
        vars.insert(key.into(), json!(count));
      }
    }
  }
  replay(ctx, page, pre, vars).await
}

/// Replay one operation of `page` with `vars` over its variables; the
/// stitched result data.
async fn replay(
  ctx: &Ctx,
  page: &Page,
  pre: &Preload,
  vars: serde_json::Map<String, Value>,
) -> Result<Value> {
  let tokens = &page.tokens;
  if !tokens.usable() {
    return Err(Error::upstream(format!(
      "the page carried no request token, so {} cannot be continued",
      pre.op
    )));
  }
  let mut variables = pre.variables.clone();
  variables.extend(vars);
  let resp = ctx
    .http
    .post(format!("{HOME}/api/graphql/"))
    .header("accept", "*/*")
    .header("accept-language", "en-US,en;q=0.9")
    .header("origin", HOME)
    .header("referer", &page.final_url)
    .header("sec-fetch-dest", "empty")
    .header("sec-fetch-mode", "cors")
    .header("sec-fetch-site", "same-origin")
    .header("x-fb-lsd", &tokens.lsd)
    .header("x-fb-friendly-name", &pre.op)
    .header("x-asbd-id", "359341")
    .form(form(tokens, pre, variables))
    .send()
    .await?;
  if !resp.status.is_success() {
    return Err(atp_core::http::status_error(resp.status, &resp.text()));
  }
  stream(&pre.op, &resp.text())
}

/// The form the site posts for an operation.
fn form(
  tokens: &Tokens,
  pre: &Preload,
  variables: serde_json::Map<String, Value>,
) -> Vec<(&'static str, String)> {
  let mut form: Vec<(&str, String)> = vec![
    ("av", tokens.user_id.clone()),
    ("__user", tokens.user_id.clone()),
    ("__a", "1".into()),
    ("__req", "1".into()),
    ("__hs", tokens.haste.clone()),
    ("dpr", "1".into()),
    ("__ccg", "EXCELLENT".into()),
    ("__rev", tokens.rev.clone()),
    ("__hsi", tokens.hsi.clone()),
    ("__comet_req", "15".into()),
    ("lsd", tokens.lsd.clone()),
    ("jazoest", tokens.jazoest()),
    ("__spin_r", tokens.rev.clone()),
    ("__spin_b", tokens.spin_b.clone()),
    ("__spin_t", tokens.spin_t.clone()),
    ("fb_api_caller_class", "RelayModern".into()),
    ("fb_api_req_friendly_name", pre.op.clone()),
    ("variables", Value::Object(variables).to_string()),
    ("server_timestamps", "true".into()),
    ("doc_id", pre.doc_id.clone()),
  ];
  if !tokens.dtsg.is_empty() {
    form.push(("fb_dtsg", tokens.dtsg.clone()));
  }
  form
}

/// A replay's answer: one JSON object per line (a root and deferred
/// fragments), stitched like a page's payloads.
fn stream(op: &str, body: &str) -> Result<Value> {
  let mut root: Option<Value> = None;
  let mut fragments: Vec<Value> = Vec::new();
  for line in body.lines() {
    let line = line.trim().trim_start_matches("for (;;);");
    if line.is_empty() {
      continue;
    }
    let Ok(v) = serde_json::from_str::<Value>(line) else {
      continue;
    };
    refused(op, &v)?;
    if v.get("label").is_some() && v.get("path").is_some() {
      fragments.push(v);
    } else if root.is_none() && v.get("data").is_some_and(Value::is_object) {
      root = v.get("data").cloned();
    }
  }
  let mut root = root.ok_or_else(|| Error::upstream(format!("{op} returned nothing")))?;
  for fragment in fragments {
    let (Some(path), Some(data)) = (
      fragment.get("path").and_then(Value::as_array),
      fragment.get("data"),
    ) else {
      continue;
    };
    if let Some(node) = relay::walk(&mut root, path) {
      relay::merge(node, data.clone());
    }
  }
  Ok(root)
}

/// An error line that ends the replay.
fn refused(op: &str, v: &Value) -> Result<()> {
  let Some(err) = v.list("errors").first() else {
    return Ok(());
  };
  let code = err.i64("code").unwrap_or(0);
  let message = err
    .str("description")
    .or_else(|| err.str("message"))
    .unwrap_or_default();
  // 1675004 says "Rate limit exceeded" but refuses logged-out callers.
  if code == 1_675_004 {
    return Err(
      Error::auth(format!(
        "Facebook continues {op} only for logged-in visitors"
      ))
      .with_hint("log in with `atp facebook login --browser` to read further"),
    );
  }
  if v.get("data").is_none_or(Value::is_null) {
    return Err(Error::upstream(format!("{op}: {message} (code {code})")));
  }
  Ok(())
}

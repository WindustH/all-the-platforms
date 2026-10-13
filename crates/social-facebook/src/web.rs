//! facebook.com as a browser sees it: page fetches (the server checks for a
//! navigation's `Sec-Fetch-*` headers, not the user agent) and replays of the
//! GraphQL operations a page ran, with only the cursor and count changed.
//!
//! Logged out, most reads work from the first page; spending a cursor
//! (timelines, more comments) is refused until a session (`c_user` + `xs`)
//! is present. A signed-out page that does not exist still answers 200, so
//! "not found" is decided by which operations came back.

use std::collections::BTreeMap;

use social_core::{Ctx, Error, ErrorCode, Result, Value, ValueExt, json};
use url::Url;

use crate::relay::{self, COUNT_KEYS, CURSOR_KEYS, Preload, Tokens};

pub const HOME: &str = "https://www.facebook.com";

/// A fetched Comet page with what it shipped.
pub struct Page {
  pub url: String,
  pub final_url: String,
  pub status: u16,
  pub html: String,
  pub docs: BTreeMap<String, Value>,
  pub preloads: Vec<Preload>,
  pub tokens: Tokens,
  pub head: BTreeMap<String, String>,
}

impl Page {
  pub fn doc(&self, op: &str) -> Option<&Value> {
    self.docs.get(op)
  }

  pub fn preload(&self, op: &str) -> Option<&Preload> {
    self.preloads.iter().find(|p| p.op == op)
  }

  /// The first paged operation whose name contains one of `parts`.
  pub fn paged(&self, parts: &[&str]) -> Option<&Preload> {
    self
      .preloads
      .iter()
      .find(|p| p.has_cursor() && parts.iter().any(|w| p.op.contains(w)))
  }

  /// A login wall, Facebook's security interstitial, or none of `wanted`.
  pub fn check(&self, what: &str, wanted: &[&str]) -> Result<()> {
    if self.html.contains("blocked by our security systems") {
      return Err(
        Error::new(
          ErrorCode::VerificationRequired,
          format!("Facebook blocked the request for {what}"),
        )
        .with_hint("wait a while, or log in (`social facebook login --browser`)"),
      );
    }
    let wall = self.final_url.contains("/login") || self.final_url.contains("/checkpoint");
    let only_login = !self.docs.is_empty()
      && self
        .docs
        .keys()
        .all(|op| op == "useCometLogInFormQuery" || op == "CometHomeRootQuery");
    if wall || only_login {
      return Err(
        Error::auth(format!("Facebook shows {what} only to logged-in visitors")).with_hint(
          "log in with `social facebook login --browser` (or `--cookie 'c_user=…; xs=…'`)",
        ),
      );
    }
    if wanted.is_empty() || wanted.iter().any(|op| self.docs.contains_key(*op)) {
      return Ok(());
    }
    let reason = if self.final_url.trim_end_matches('/') == HOME
      || self.final_url.contains("content_not_found")
    {
      format!("the page redirected to {}", self.final_url)
    } else if self.status >= 400 {
      format!("HTTP {}", self.status)
    } else if self.docs.is_empty() {
      "the page carried no data".into()
    } else {
      format!(
        "the page shipped {} but not what is needed",
        self.docs.keys().cloned().collect::<Vec<_>>().join(", ")
      )
    };
    Err(Error::not_found(format!("{what} not found ({reason})")))
  }
}

/// Fetch a facebook.com page the way a navigation does.
pub async fn page(ctx: &Ctx, url: &str) -> Result<Page> {
  let url = canonical(url);
  let resp = ctx
    .http
    .get(&url)
    .header(
      "accept",
      "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
    )
    .header("accept-language", "en-US,en;q=0.9")
    .header("sec-fetch-dest", "document")
    .header("sec-fetch-mode", "navigate")
    .header("sec-fetch-site", "none")
    .header("upgrade-insecure-requests", "1")
    .send()
    .await?;
  let html = resp.text();
  let blocks = relay::blocks(&html);
  Ok(Page {
    url,
    final_url: resp.url.clone(),
    status: resp.status.as_u16(),
    docs: relay::documents(&blocks),
    preloads: relay::preloads(&blocks),
    tokens: relay::tokens(&html),
    head: head(&html),
    html,
  })
}

/// Replay one operation of `page` with `vars` over its variables; the
/// stitched result data.
pub async fn replay(
  ctx: &Ctx,
  page: &Page,
  pre: &Preload,
  vars: serde_json::Map<String, Value>,
) -> Result<Value> {
  let t = &page.tokens;
  if !t.usable() {
    return Err(Error::upstream(format!(
      "the page carried no request token, so {} cannot be continued",
      pre.op
    )));
  }
  let mut variables = pre.variables.clone();
  variables.extend(vars);
  let mut form: Vec<(&str, String)> = vec![
    ("av", t.user_id.clone()),
    ("__user", t.user_id.clone()),
    ("__a", "1".into()),
    ("__req", "1".into()),
    ("__hs", t.haste.clone()),
    ("dpr", "1".into()),
    ("__ccg", "EXCELLENT".into()),
    ("__rev", t.rev.clone()),
    ("__hsi", t.hsi.clone()),
    ("__comet_req", "15".into()),
    ("lsd", t.lsd.clone()),
    ("jazoest", t.jazoest()),
    ("__spin_r", t.rev.clone()),
    ("__spin_b", t.spin_b.clone()),
    ("__spin_t", t.spin_t.clone()),
    ("fb_api_caller_class", "RelayModern".into()),
    ("fb_api_req_friendly_name", pre.op.clone()),
    ("variables", Value::Object(variables).to_string()),
    ("server_timestamps", "true".into()),
    ("doc_id", pre.doc_id.clone()),
  ];
  if !t.dtsg.is_empty() {
    form.push(("fb_dtsg", t.dtsg.clone()));
  }
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
    .header("x-fb-lsd", &t.lsd)
    .header("x-fb-friendly-name", &pre.op)
    .header("x-asbd-id", "359341")
    .form(form)
    .send()
    .await?;
  if !resp.status.is_success() {
    return Err(social_core::http::status_error(resp.status, &resp.text()));
  }
  stream(&pre.op, &resp.text())
}

/// Replay with the cursor moved to `cursor` and the count set to `count`.
pub async fn advance(
  ctx: &Ctx,
  page: &Page,
  pre: &Preload,
  cursor: &str,
  count: usize,
) -> Result<Value> {
  let mut vars = serde_json::Map::new();
  for k in CURSOR_KEYS {
    if pre.variables.contains_key(k) {
      vars.insert(k.into(), json!(cursor));
    }
  }
  if vars.is_empty() {
    return Err(Error::unsupported("paging this list"));
  }
  if count > 0 {
    for k in COUNT_KEYS {
      if pre.variables.contains_key(k) {
        vars.insert(k.into(), json!(count));
      }
    }
  }
  replay(ctx, page, pre, vars).await
}

/// A replay's answer: one JSON object per line (a root and deferred
/// fragments), stitched like a page's payloads.
fn stream(op: &str, body: &str) -> Result<Value> {
  let mut root: Option<Value> = None;
  let mut frags: Vec<Value> = Vec::new();
  for line in body.lines() {
    let line = line.trim().trim_start_matches("for (;;);");
    if line.is_empty() {
      continue;
    }
    let Ok(v) = serde_json::from_str::<Value>(line) else {
      continue;
    };
    if let Some(err) = v.list("errors").first() {
      let code = err.i64("code").unwrap_or(0);
      let msg = err
        .str("description")
        .or_else(|| err.str("message"))
        .unwrap_or_default();
      // 1675004 says "Rate limit exceeded" but refuses logged-out callers.
      if code == 1_675_004 {
        return Err(
          Error::auth(format!(
            "Facebook continues {op} only for logged-in visitors"
          ))
          .with_hint("log in with `social facebook login --browser` to read further"),
        );
      }
      if v.get("data").is_none_or(Value::is_null) {
        return Err(Error::upstream(format!("{op}: {msg} (code {code})")));
      }
    }
    if v.get("label").is_some() && v.get("path").is_some() {
      frags.push(v);
    } else if root.is_none() && v.get("data").is_some_and(Value::is_object) {
      root = v.get("data").cloned();
    }
  }
  let mut root = root.ok_or_else(|| Error::upstream(format!("{op} returned nothing")))?;
  for f in frags {
    let (Some(path), Some(data)) = (f.get("path").and_then(Value::as_array), f.get("data")) else {
      continue;
    };
    if let Some(node) = relay::walk(&mut root, path) {
      relay::merge(node, data.clone());
    }
  }
  Ok(root)
}

/// The `<meta property|name content>` pairs of the head.
fn head(html: &str) -> BTreeMap<String, String> {
  let end = html.find("</head>").unwrap_or(html.len().min(200_000));
  let head = &html[..end];
  let mut out = BTreeMap::new();
  let mut i = 0;
  while let Some(j) = head[i..].find("<meta ") {
    let start = i + j;
    let Some(len) = head[start..].find('>') else {
      break;
    };
    let tag = &head[start..start + len];
    i = start + len;
    let key = attr(tag, "property").or_else(|| attr(tag, "name"));
    if let (Some(k), Some(v)) = (key, attr(tag, "content")) {
      out.entry(k).or_insert(v);
    }
  }
  out
}

fn attr(tag: &str, name: &str) -> Option<String> {
  let k = tag.find(&format!("{name}=\""))? + name.len() + 2;
  let rest = &tag[k..];
  Some(entities(&rest[..rest.find('"')?]))
}

fn entities(s: &str) -> String {
  s.replace("&quot;", "\"")
    .replace("&#039;", "'")
    .replace("&#x27;", "'")
    .replace("&lt;", "<")
    .replace("&gt;", ">")
    .replace("&amp;", "&")
}

/// facebook.com links on `www`, https, without per-render tracking.
pub fn canonical(raw: &str) -> String {
  let Ok(mut u) = Url::parse(raw) else {
    return raw.to_owned();
  };
  let host = u.host_str().unwrap_or_default().to_ascii_lowercase();
  if !is_facebook(&host) {
    return raw.to_owned();
  }
  let _ = u.set_host(Some("www.facebook.com"));
  let _ = u.set_scheme("https");
  let drop = [
    "__tn__",
    "fref",
    "mibextid",
    "rdid",
    "ref",
    "refsrc",
    "hc_ref",
    "share_url",
    "eav",
    "paipv",
    "_rdc",
    "_rdr",
    "sfnsn",
    "notif_id",
    "notif_t",
  ];
  let kept: Vec<(String, String)> = u
    .query_pairs()
    .filter(|(k, _)| !drop.contains(&k.as_ref()) && !k.starts_with("__cft__"))
    .map(|(k, v)| (k.into_owned(), v.into_owned()))
    .collect();
  if kept.is_empty() {
    u.set_query(None);
  } else {
    u.query_pairs_mut().clear().extend_pairs(kept);
  }
  u.set_fragment(None);
  u.into()
}

pub fn is_facebook(host: &str) -> bool {
  matches!(
    host,
    "www.facebook.com"
      | "web.facebook.com"
      | "m.facebook.com"
      | "mbasic.facebook.com"
      | "facebook.com"
      | "free.facebook.com"
      | "d.facebook.com"
  )
}

/// The destination behind an `l.facebook.com` redirect.
pub fn unshim(raw: &str) -> String {
  let Ok(u) = Url::parse(raw) else {
    return raw.to_owned();
  };
  let host = u.host_str().unwrap_or_default();
  if !matches!(
    host,
    "l.facebook.com" | "lm.facebook.com" | "l.messenger.com"
  ) {
    return raw.to_owned();
  }
  u.query_pairs()
    .find(|(k, _)| k == "u")
    .map(|(_, v)| v.into_owned())
    .unwrap_or_else(|| raw.to_owned())
}

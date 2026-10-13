//! What a facebook.com (Comet) page ships besides its markup.
//!
//! The page's Relay store arrives as `<script type="application/json"
//! data-sjs>` blocks. Query results inside them are
//! `["RelayPrefetchedStreamCache", "next", [], [preloader_id, {"__bbox":
//! {"result": …}}]]` entries: one root per operation plus deferred fragments
//! (`label` + `path` into the root), stitched here into the document the
//! browser ends up with. The same blocks describe each operation the page ran
//! (`queryID` = persisted doc id, `variables`), which is what paging replays,
//! and the HTML carries the request tokens (`lsd`, revision, `fb_dtsg` when
//! logged in). Approach and field paths after tamnd/facebook-cli.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use regex::Regex;
use social_core::Value;

/// Parsed bodies of the page's `data-sjs` JSON blocks.
pub fn blocks(html: &str) -> Vec<Value> {
  let mut out = Vec::new();
  let mut i = 0;
  while let Some(j) = html[i..].find("<script") {
    let start = i + j;
    let Some(gt) = html[start..].find('>') else {
      break;
    };
    let tag = &html[start..start + gt];
    i = start + gt + 1;
    if !tag.contains("type=\"application/json\"") || !tag.contains("data-sjs") {
      continue;
    }
    let Some(end) = html[i..].find("</script>") else {
      break;
    };
    let body = &html[i..i + end];
    i += end + "</script>".len();
    // A block shorter than the length the server announced was cut off.
    if let Some(claimed) = attr_len(tag)
      && claimed != body.len()
    {
      continue;
    }
    if let Ok(v) = serde_json::from_str(body) {
      out.push(v);
    }
  }
  out
}

fn attr_len(tag: &str) -> Option<usize> {
  let k = tag.find("data-content-len=\"")? + "data-content-len=\"".len();
  let rest = &tag[k..];
  rest[..rest.find('"')?].parse().ok()
}

/// One Relay result: a root (no label) or a deferred fragment.
struct Payload {
  op: String,
  label: Option<String>,
  path: Option<Vec<Value>>,
  data: Value,
  sequence: i64,
}

/// Stitched query results of the page, by operation name.
pub fn documents(blocks: &[Value]) -> BTreeMap<String, Value> {
  let mut payloads = Vec::new();
  for b in blocks {
    collect(b, &mut payloads);
  }
  stitch_all(payloads)
}

fn collect(v: &Value, out: &mut Vec<Payload>) {
  match v {
    Value::Object(m) => m.values().for_each(|s| collect(s, out)),
    Value::Array(a) => {
      if let Some(p) = entry(a) {
        out.push(p);
        return;
      }
      a.iter().for_each(|s| collect(s, out));
    }
    _ => {}
  }
}

/// A `["RelayPrefetchedStreamCache", "next", [], [id, {"__bbox": …}]]` entry.
fn entry(a: &[Value]) -> Option<Payload> {
  let name = a.first()?.as_str()?;
  if name.split('@').next() != Some("RelayPrefetchedStreamCache") {
    return None;
  }
  let args = a.get(3)?.as_array()?;
  let id = args.first()?.as_str().unwrap_or_default();
  let bbox = args.get(1)?.get("__bbox")?;
  let result = bbox.get("result")?;
  Some(Payload {
    op: op_from_preloader(id),
    label: result
      .get("label")
      .and_then(Value::as_str)
      .map(str::to_owned),
    path: result.get("path").and_then(Value::as_array).cloned(),
    data: result.get("data").cloned().unwrap_or(Value::Null),
    sequence: bbox
      .get("sequence_number")
      .and_then(Value::as_i64)
      .unwrap_or(0),
  })
}

/// `adp_ProfileCometHeaderQueryRelayPreloader_6a69…` → `ProfileCometHeaderQuery`.
fn op_from_preloader(id: &str) -> String {
  let s = id.strip_prefix("adp_").unwrap_or(id);
  match s.find("RelayPreloader") {
    Some(i) if i > 0 => s[..i].to_owned(),
    _ => s.to_owned(),
  }
}

fn stitch_all(payloads: Vec<Payload>) -> BTreeMap<String, Value> {
  let mut by_op: BTreeMap<String, Vec<Payload>> = BTreeMap::new();
  for p in payloads {
    by_op.entry(p.op.clone()).or_default().push(p);
  }
  by_op
    .into_iter()
    .filter_map(|(op, ps)| stitch(ps).map(|d| (op, d)))
    .collect()
}

/// The root payload with every fragment merged in at its path, in sequence order.
fn stitch(mut ps: Vec<Payload>) -> Option<Value> {
  let root_at = ps
    .iter()
    .position(|p| p.label.is_none() && p.path.is_none() && p.data.is_object())?;
  let mut root = ps.remove(root_at).data;
  ps.sort_by_key(|p| p.sequence);
  for f in ps {
    if !f.data.is_object() {
      continue;
    }
    let path = f.path.unwrap_or_default();
    if let Some(node) = walk(&mut root, &path) {
      merge(node, f.data);
    }
  }
  Some(root)
}

pub fn walk<'a>(root: &'a mut Value, path: &[Value]) -> Option<&'a mut Value> {
  let mut cur = root;
  for step in path {
    cur = match step {
      Value::String(k) => cur.get_mut(k.as_str())?,
      Value::Number(n) => cur.get_mut(n.as_u64()? as usize)?,
      _ => return None,
    };
  }
  cur.is_object().then_some(cur)
}

/// A fragment over a node: maps merge recursively, everything else replaces.
pub fn merge(node: &mut Value, frag: Value) {
  let (Value::Object(into), Value::Object(from)) = (node, frag) else {
    return;
  };
  for (k, v) in from {
    match (into.get_mut(&k), v) {
      (Some(existing @ Value::Object(_)), sub @ Value::Object(_)) => merge(existing, sub),
      (_, v) => {
        into.insert(k, v);
      }
    }
  }
}

/// One operation the page ran, replayable with another cursor.
#[derive(Debug, Clone)]
pub struct Preload {
  pub op: String,
  pub doc_id: String,
  pub variables: serde_json::Map<String, Value>,
}

impl Preload {
  pub fn has_cursor(&self) -> bool {
    CURSOR_KEYS.iter().any(|k| self.variables.contains_key(*k))
  }
}

/// Names a paged operation spells its position with.
pub const CURSOR_KEYS: [&str; 2] = ["cursor", "after"];
pub const COUNT_KEYS: [&str; 2] = ["count", "first"];

/// Every operation descriptor in the blocks (first one per name wins).
pub fn preloads(blocks: &[Value]) -> Vec<Preload> {
  let mut out: Vec<Preload> = Vec::new();
  for b in blocks {
    collect_preloads(b, &mut out);
  }
  out
}

fn collect_preloads(v: &Value, out: &mut Vec<Preload>) {
  match v {
    Value::Object(m) => {
      if let Some(id) = m
        .get("queryID")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
      {
        let mut op = m
          .get("queryName")
          .and_then(Value::as_str)
          .unwrap_or_default()
          .to_owned();
        if op.is_empty() {
          op = op_from_preloader(
            m.get("preloaderID")
              .and_then(Value::as_str)
              .unwrap_or_default(),
          );
        }
        if !op.is_empty() && !out.iter().any(|p| p.op == op) {
          out.push(Preload {
            op,
            doc_id: id.to_owned(),
            variables: m
              .get("variables")
              .and_then(Value::as_object)
              .cloned()
              .unwrap_or_default(),
          });
        }
        return;
      }
      m.values().for_each(|s| collect_preloads(s, out));
    }
    Value::Array(a) => a.iter().for_each(|s| collect_preloads(s, out)),
    _ => {}
  }
}

/// Request tokens a page carries: none secret, all needed to replay.
#[derive(Debug, Clone, Default)]
pub struct Tokens {
  pub lsd: String,
  pub rev: String,
  pub spin_b: String,
  pub spin_t: String,
  pub haste: String,
  pub hsi: String,
  /// `fb_dtsg`, present only with a session.
  pub dtsg: String,
  /// `0` logged out.
  pub user_id: String,
}

macro_rules! re {
  ($name:ident, $pat:literal) => {
    static $name: LazyLock<Regex> = LazyLock::new(|| Regex::new($pat).expect("valid"));
  };
}
re!(LSD, r#""LSD",\[\],\{"token":"([^"]+)""#);
re!(SPIN_R, r#""__spin_r":(\d+)"#);
re!(SPIN_B, r#""__spin_b":"([^"]+)""#);
re!(SPIN_T, r#""__spin_t":(\d+)"#);
re!(HASTE, r#""haste_session":"([^"]+)""#);
re!(HSI, r#""hsi":"([^"]+)""#);
re!(DTSG, r#""DTSGInitialData",\[\],\{"token":"([^"]+)""#);
re!(USER_ID, r#""USER_ID":"(\d+)""#);
re!(REVISION, r#""server_revision":(\d+)"#);

pub fn tokens(html: &str) -> Tokens {
  let first = |re: &Regex| {
    re.captures(html)
      .map(|m| m[1].to_owned())
      .unwrap_or_default()
  };
  let mut t = Tokens {
    lsd: first(&LSD),
    rev: first(&SPIN_R),
    spin_b: first(&SPIN_B),
    spin_t: first(&SPIN_T),
    haste: first(&HASTE),
    hsi: first(&HSI),
    dtsg: first(&DTSG),
    user_id: first(&USER_ID),
  };
  if t.rev.is_empty() {
    t.rev = first(&REVISION);
  }
  if t.user_id.is_empty() {
    t.user_id = "0".into();
  }
  t
}

impl Tokens {
  pub fn usable(&self) -> bool {
    !self.lsd.is_empty() && !self.rev.is_empty()
  }

  /// `2` followed by the sum of the token's bytes, as the site computes it.
  pub fn jazoest(&self) -> String {
    let token = if self.dtsg.is_empty() {
      &self.lsd
    } else {
      &self.dtsg
    };
    format!("2{}", token.bytes().map(u32::from).sum::<u32>())
  }
}

// ── walking the irregular tree ─────────────────────────────────────────────

/// The first object in the subtree having `key` set (non-null), depth first,
/// keys in sorted order so the same page always yields the same answer.
pub fn find_key<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
  match v {
    Value::Object(m) => {
      if m.get(key).is_some_and(|x| !x.is_null()) {
        return Some(v);
      }
      let mut keys: Vec<&String> = m.keys().collect();
      keys.sort();
      keys.into_iter().find_map(|k| find_key(&m[k], key))
    }
    Value::Array(a) => a.iter().find_map(|s| find_key(s, key)),
    _ => None,
  }
}

/// `node`s of a Relay connection (`{"edges": [{"node": …}]}`).
pub fn edges(v: &Value) -> Vec<&Value> {
  v.get("edges")
    .and_then(Value::as_array)
    .map(|a| a.iter().filter_map(|e| e.get("node")).collect())
    .unwrap_or_default()
}

/// Every object in the subtree for which `pred` holds, outermost first; the
/// match's own subtree is not searched again.
pub fn find_all<'a>(v: &'a Value, pred: &dyn Fn(&Value) -> bool, out: &mut Vec<&'a Value>) {
  match v {
    Value::Object(m) => {
      if pred(v) {
        out.push(v);
        return;
      }
      let mut keys: Vec<&String> = m.keys().collect();
      keys.sort();
      for k in keys {
        find_all(&m[k], pred, out);
      }
    }
    Value::Array(a) => a.iter().for_each(|s| find_all(s, pred, out)),
    _ => {}
  }
}

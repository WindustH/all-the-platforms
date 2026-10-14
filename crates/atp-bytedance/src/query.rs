//! Query strings exactly as the signatures see them.

/// Percent-encoding of everything but ASCII alphanumerics and `*-._`
/// (Python's `quote(v, safe="*-._")`), the form the signed bytes take.
pub fn encode(s: &str) -> String {
  let mut out = String::with_capacity(s.len());
  for b in s.bytes() {
    if b.is_ascii_alphanumeric() || matches!(b, b'*' | b'-' | b'.' | b'_') {
      out.push(char::from(b));
    } else {
      out.push_str(&format!("%{b:02X}"));
    }
  }
  out
}

pub fn query_string(pairs: &[(String, String)]) -> String {
  pairs
    .iter()
    .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
    .collect::<Vec<_>>()
    .join("&")
}

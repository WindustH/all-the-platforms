//! Reading values out of the inline scripts of mp.weixin pages without
//! running them: `var name = "a" || "b";`-style assignments, JS string
//! escapes and HTML entities.

/// The first non-empty string literal assigned to `var <name>` (or to
/// `<name>` after `window.`), unescaped.
pub fn var(html: &str, name: &str) -> Option<String> {
  for prefix in ["var ", "window."] {
    let needle = format!("{prefix}{name}");
    let mut from = 0;
    while let Some(pos) = html[from..].find(&needle) {
      let start = from + pos + needle.len();
      from = start;
      let rest = html[start..].trim_start();
      // `var name` must be followed by `=`, not by more of a longer name.
      let Some(expr) = rest.strip_prefix('=') else {
        continue;
      };
      if let Some(v) = literals(statement(expr))
        .into_iter()
        .find(|s| !s.is_empty())
      {
        return Some(v);
      }
    }
  }
  None
}

/// `var <name>` decoded as text: JS escapes, then HTML entities.
pub fn text(html: &str, name: &str) -> Option<String> {
  var(html, name)
    .map(|v| entities(&v).trim().to_owned())
    .filter(|v| !v.is_empty())
}

/// The expression up to the `;` or line end that closes the statement.
fn statement(s: &str) -> &str {
  let mut quote = None;
  let mut escaped = false;
  for (i, c) in s.char_indices() {
    match quote {
      Some(q) => {
        if escaped {
          escaped = false;
        } else if c == '\\' {
          escaped = true;
        } else if c == q {
          quote = None;
        }
      }
      None => match c {
        '"' | '\'' => quote = Some(c),
        ';' | '\n' => return &s[..i],
        _ => {}
      },
    }
  }
  s
}

/// Every string literal in a JS expression, unescaped.
pub fn literals(expr: &str) -> Vec<String> {
  let mut out = Vec::new();
  let mut chars = expr.chars();
  while let Some(c) = chars.next() {
    if c != '"' && c != '\'' {
      continue;
    }
    let mut raw = String::new();
    let mut escaped = false;
    for d in chars.by_ref() {
      if escaped {
        raw.push('\\');
        raw.push(d);
        escaped = false;
      } else if d == '\\' {
        escaped = true;
      } else if d == c {
        break;
      } else {
        raw.push(d);
      }
    }
    out.push(unescape(&raw));
  }
  out
}

/// JS string escapes: `\xNN`, `\uNNNN`, `\n`, `\/`, quotes and backslashes.
pub fn unescape(s: &str) -> String {
  let mut out = String::with_capacity(s.len());
  let mut chars = s.chars().peekable();
  while let Some(c) = chars.next() {
    if c != '\\' {
      out.push(c);
      continue;
    }
    match chars.next() {
      Some('n') => out.push('\n'),
      Some('t') => out.push('\t'),
      Some('r') => {}
      Some(x @ ('x' | 'u')) => {
        let len = if x == 'x' { 2 } else { 4 };
        let hex: String = (0..len).filter_map(|_| chars.next()).collect();
        match u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
          Some(ch) => out.push(ch),
          None => {
            out.push('\\');
            out.push(x);
            out.push_str(&hex);
          }
        }
      }
      Some(other) => out.push(other),
      None => out.push('\\'),
    }
  }
  out
}

/// The HTML entities WeChat writes into titles and names.
pub fn entities(s: &str) -> String {
  if !s.contains('&') {
    return s.to_owned();
  }
  let mut out = String::with_capacity(s.len());
  let mut rest = s;
  while let Some(pos) = rest.find('&') {
    out.push_str(&rest[..pos]);
    rest = &rest[pos..];
    let Some(end) = rest[..rest.len().min(12)].find(';') else {
      out.push('&');
      rest = &rest[1..];
      continue;
    };
    let name = &rest[1..end];
    let ch = match name {
      "amp" => Some('&'),
      "lt" => Some('<'),
      "gt" => Some('>'),
      "quot" => Some('"'),
      "apos" | "#39" => Some('\''),
      "nbsp" => Some(' '),
      "yen" => Some('¥'),
      _ => name
        .strip_prefix("#x")
        .or_else(|| name.strip_prefix("#X"))
        .and_then(|h| u32::from_str_radix(h, 16).ok())
        .or_else(|| name.strip_prefix('#').and_then(|d| d.parse().ok()))
        .and_then(char::from_u32),
    };
    match ch {
      Some(c) => {
        out.push(c);
        rest = &rest[end + 1..];
      }
      None => {
        out.push('&');
        rest = &rest[1..];
      }
    }
  }
  out.push_str(rest);
  out
}

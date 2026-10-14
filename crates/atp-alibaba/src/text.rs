//! Small text forms both sites share: scheme-less image links, prices as
//! shown, and times written in Beijing time.

use atp_core::{Value, json};

/// `//img.alicdn.com/…` with its scheme.
pub fn https(url: String) -> String {
  match url.strip_prefix("//") {
    Some(rest) => format!("https://{rest}"),
    None => url,
  }
}

/// A price as shown (`¥99.5`, `当前价 99.5`), without the currency.
pub fn price(text: Option<String>) -> Option<Value> {
  let text = text?.replace("当前价", "");
  let text = text.trim().trim_start_matches('¥').trim();
  (!text.is_empty()).then(|| json!(text))
}

/// `2024-05-03 12:30:00` or `2024-05-03`, Beijing time.
pub fn beijing(text: &str) -> Option<jiff::Timestamp> {
  let text = text.trim().replace(' ', "T");
  let text = if text.len() == 10 {
    format!("{text}T00:00:00")
  } else {
    text
  };
  format!("{text}+08:00").parse().ok()
}

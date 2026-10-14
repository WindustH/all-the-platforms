//! Small text forms both sites share: scheme-less image links, prices as
//! shown, and times written in Beijing time.

use atp_core::Price;

/// `//img.alicdn.com/…` with its scheme.
pub fn https(url: String) -> String {
  match url.strip_prefix("//") {
    Some(rest) => format!("https://{rest}"),
    None => url,
  }
}

/// A price as shown (`¥99.5`, `当前价 99.5`, `99起`, `1.2万`) with the
/// crossed-out one; wording beyond the number is kept as the label.
pub fn price(shown: Option<String>, original: Option<String>) -> Option<Price> {
  let shown = shown?;
  let (value, rest) = amount(&shown)?;
  Some(Price {
    amount: value,
    currency: "CNY".into(),
    original: original.as_deref().and_then(amount).map(|(a, _)| a),
    label: (!rest.is_empty()).then(|| shown.trim().to_owned()),
  })
}

/// The number in a shown price and whatever words came with it.
fn amount(text: &str) -> Option<(f64, String)> {
  let cleaned = text.replace(['¥', '￥'], "").replace("当前价", "");
  let cleaned = cleaned.trim();
  let start = cleaned.find(|c: char| c.is_ascii_digit())?;
  let digits: String = cleaned[start..]
    .chars()
    .take_while(|c| c.is_ascii_digit() || *c == '.')
    .collect();
  let mut value: f64 = digits.parse().ok()?;
  let after = cleaned[start + digits.len()..].trim();
  if after.starts_with('万') {
    value *= 10_000.0;
  }
  let rest = format!("{}{}", &cleaned[..start], after);
  Some((value, rest.trim().to_owned()))
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

//! The body of a draft, as given: HTML when the text starts with a tag,
//! otherwise plain text with one paragraph per line. Local images in the
//! HTML (`<img src="photo.jpg">`) and attached ones are uploaded by the
//! route that saves the draft, since articles may only show images hosted
//! by WeChat. No styling is added.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use social_core::{Error, Result};

static LOCAL_IMG: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r#"(<img\b[^>]*?\bsrc=")([^"]+)(")"#).expect("valid"));

/// A body whose images still have to be uploaded: `html` holds a
/// placeholder per entry of `images`.
pub struct Body {
  html: String,
  pub images: Vec<PathBuf>,
}

fn placeholder(i: usize) -> String {
  format!("\u{0}image-{i}\u{0}")
}

impl Body {
  /// `text` as HTML or paragraphs, with `attached` images appended as their
  /// own paragraphs.
  pub fn new(text: &str, attached: &[PathBuf], base: &Path) -> Result<Self> {
    let text = text.trim();
    let mut images = Vec::new();
    let mut html = if text.starts_with('<') {
      let mut missing = None;
      let html = LOCAL_IMG.replace_all(text, |m: &regex::Captures| {
        let src = &m[2];
        if src.starts_with("http://") || src.starts_with("https://") || src.starts_with("data:") {
          return m[0].to_owned();
        }
        let path = base.join(src);
        if !path.is_file() {
          missing.get_or_insert(path.display().to_string());
          return m[0].to_owned();
        }
        images.push(path);
        format!("{}{}{}", &m[1], placeholder(images.len() - 1), &m[3])
      });
      if let Some(path) = missing {
        return Err(Error::input(format!("image not found: {path}")));
      }
      html.into_owned()
    } else {
      text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| format!("<p>{}</p>", escape(l)))
        .collect()
    };
    for path in attached {
      images.push(path.clone());
      html += &format!("<p><img src=\"{}\"></p>", placeholder(images.len() - 1));
    }
    Ok(Self { html, images })
  }

  /// The HTML with the uploaded images' URLs in place.
  pub fn fill(&self, urls: &[String]) -> String {
    let mut out = self.html.clone();
    for (i, url) in urls.iter().enumerate() {
      out = out.replace(&placeholder(i), url);
    }
    out
  }
}

fn escape(s: &str) -> String {
  s.replace('&', "&amp;")
    .replace('<', "&lt;")
    .replace('>', "&gt;")
}

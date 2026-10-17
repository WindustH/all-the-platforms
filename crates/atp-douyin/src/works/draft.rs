//! What a draft becomes on Douyin, what the web form refuses, and the
//! summary the person says yes to before anything is uploaded.

use std::path::Path;

use atp_core::cli::confirm;
use atp_core::output::note;
use atp_core::{Draft, Error, Result};

use super::POST_PAGE;

/// The limits the web form holds a post to (JavaScript string lengths).
pub(super) const VIDEO_TITLE_MAX: usize = 30;
pub(super) const IMAGE_TITLE_MAX: usize = 20;
const ARTICLE_TITLE_MAX: usize = 30;
const ARTICLE_SUMMARY_MAX: usize = 30;
pub(super) const TEXT_MAX: usize = 1000;
const ARTICLE_MIN: usize = 100;
/// Scheduled posts go out 2 hours to 14 days ahead.
const SCHEDULE_MIN: i64 = 2 * 3600;
const SCHEDULE_MAX: i64 = 14 * 86_400;

/// What a draft publishes.
pub(super) enum Kind<'a> {
  Video(&'a Path),
  Images,
  Article(&'a Path),
}

impl Kind<'_> {
  pub(super) fn of(draft: &Draft) -> Result<Kind<'_>> {
    match (&draft.video, &draft.article, draft.images.is_empty()) {
      (Some(video), None, true) => Ok(Kind::Video(video)),
      (None, Some(article), true) => Ok(Kind::Article(article)),
      (None, None, false) => Ok(Kind::Images),
      (None, None, true) => Err(Error::input(
        "a Douyin post is a video (--video), images (-i) or an article (--article); \
         text-only posts are made in the app",
      )),
      _ => Err(Error::input("give one of --video, -i or --article")),
    }
  }

  pub(super) fn name(&self) -> &'static str {
    match self {
      Kind::Video(_) => "video",
      Kind::Images => "image post",
      Kind::Article(_) => "article",
    }
  }

  /// The form the page publishes it from (the referer of `create_v2`).
  pub(super) fn page(&self) -> String {
    let form = match self {
      Kind::Video(_) => "video",
      Kind::Images => "image",
      Kind::Article(_) => "article",
    };
    format!("{POST_PAGE}/{form}?enter_from=publish_page")
  }
}

/// What the web form refuses, before anything is sent.
pub(super) fn check(draft: &Draft, kind: &Kind) -> Result<()> {
  if draft.reply_to.is_some() || draft.quote.is_some() {
    return Err(Error::unsupported("post --reply-to / --quote"));
  }
  let title = utf16(draft.title.as_deref().unwrap_or_default().trim());
  let text = utf16(&draft.text);
  let fail = |message: &str| Err(Error::input(message.to_owned()));
  match kind {
    Kind::Video(_) if title > VIDEO_TITLE_MAX => fail("a video title takes at most 30 characters"),
    Kind::Images if title > IMAGE_TITLE_MAX => {
      fail("an image post's title takes at most 20 characters")
    }
    Kind::Images if draft.cover.is_some() => {
      fail("an image post's cover is its first image; leave out --cover")
    }
    Kind::Video(_) if draft.music.is_some() => fail("--music is for image posts and articles"),
    Kind::Article(_) if title == 0 || title > ARTICLE_TITLE_MAX => {
      fail("an article needs a --title of at most 30 characters")
    }
    Kind::Article(_) if text > ARTICLE_SUMMARY_MAX => {
      fail("an article's summary (TEXT) takes at most 30 characters")
    }
    Kind::Article(_) if draft.cover.is_none() => fail("an article needs a cover (--cover)"),
    Kind::Article(_) if draft.ai_generated => fail("Douyin's article form has no AI declaration"),
    _ if text > TEXT_MAX => fail("the text takes at most 1000 characters"),
    _ => check_settings(draft),
  }
}

fn check_settings(draft: &Draft) -> Result<()> {
  if draft.collection.is_some() && !matches!(draft.visibility.as_deref(), None | Some("public")) {
    return Err(Error::input("posts in a collection must be public"));
  }
  if let Some(when) = draft.schedule {
    let ahead = when.as_second() - jiff::Timestamp::now().as_second();
    if !(SCHEDULE_MIN..=SCHEDULE_MAX).contains(&ahead) {
      return Err(Error::input("--schedule must be 2 hours to 14 days ahead"));
    }
  }
  Ok(())
}

/// The article's Markdown; the page holds it to 100 characters at least
/// and uploads its pictures itself, which ATP does not.
pub(super) fn article_body(path: &Path) -> Result<String> {
  let text = std::fs::read_to_string(path)
    .map_err(|e| Error::input(format!("cannot read {}: {e}", path.display())))?;
  if text.contains("![") {
    return Err(Error::input(
      "pictures inside an article are not supported; remove the ![...](...) images",
    ));
  }
  if text.chars().filter(|c| !c.is_whitespace()).count() < ARTICLE_MIN {
    return Err(Error::input(
      "an article's body takes at least 100 characters",
    ));
  }
  Ok(text)
}

/// Show what goes out and ask (or take `--yes`).
pub(super) fn ask(draft: &Draft, kind: &Kind) -> Result<()> {
  let mut lines = vec![format!("Douyin {}:", kind.name())];
  let mut line = |label: &str, value: String| lines.push(format!("  {label:<11} {value}"));
  line(
    "title",
    draft.title.clone().unwrap_or_else(|| "(none)".into()),
  );
  if !draft.text.is_empty() {
    line("text", atp_core::text::truncate(&draft.text, 60));
  }
  if !draft.topics.is_empty() {
    line("hashtags", draft.topics.join(" "));
  }
  line(
    "visibility",
    draft.visibility.clone().unwrap_or_else(|| "public".into()),
  );
  line(
    "AI label",
    if draft.ai_generated {
      "yes (内容由AI生成)"
    } else {
      "no"
    }
    .into(),
  );
  if let Some(when) = draft.schedule {
    line("schedule", atp_core::text::fmt_time(when));
  }
  for (label, value) in [("collection", &draft.collection), ("music", &draft.music)] {
    if let Some(value) = value {
      line(label, value.clone());
    }
  }
  let files = [&draft.video, &draft.article, &draft.cover]
    .into_iter()
    .flatten();
  for path in files.chain(&draft.images) {
    line("file", format!("{} ({})", path.display(), size_of(path)));
  }
  note(&lines.join("\n"));
  confirm(draft.yes, "publish this on Douyin")
}

fn size_of(path: &Path) -> String {
  let bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
  if bytes >= 1 << 20 {
    format!("{:.1} MB", bytes as f64 / f64::from(1 << 20))
  } else {
    format!("{:.0} KB", (bytes as f64 / 1024.0).ceil())
  }
}

pub(super) fn utf16(s: &str) -> usize {
  s.encode_utf16().count()
}

//! Public article pages, read anonymously.
//!
//! The page carries everything as inline script variables (`msg_title`,
//! `nickname`, `ct` ...), the body in `#js_content`, video qualities in
//! `videoPageInfos` and audio as `<mpvoice voice_encode_fileid>`. WeChat
//! answers desktop-Chrome user agents and long links without `chksm` with a
//! captcha page ("环境异常"), so the platform sends a Safari user agent.

use std::sync::LazyLock;

use regex::Regex;
use social_core::text::{from_secs, html_to_text};
use social_core::{Ctx, Error, ErrorCode, Media, MediaKind, Post, Result, User, Value, json};
use url::Url;

use crate::js::{entities, literals, text, var};
use crate::refs;

static IMAGE: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r#"data-src="(https?://mmbiz\.(?:qpic|qlogo)\.cn/[^"]+)""#).expect("valid")
});
static QUALITY: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(
    r"(?s)filesize:\s*'(\d+)'.*?height:\s*'(\d+)'.*?url:\s*\('([^']+)'\).*?width:\s*'(\d+)'",
  )
  .expect("valid")
});
static DURATION: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"duration_ms:\s*'(\d+)'").expect("valid"));
static VOICE: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r#"voice_encode_fileid="([^"]+)""#).expect("valid"));
static MSG_TITLE: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r#"weui-msg__title[^>]*>\s*([^<]+?)\s*<"#).expect("valid"));
static ALBUM: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"(?s)title:\s*'([^']*)'.{0,400}?albumId:\s*'(\d+)'").expect("valid")
});
static META_TITLE: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r#"<meta property="og:title" content="([^"]*)""#).expect("valid"));
static PICTURE: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"cdn_url:\s*'(https?://mmbiz[^']+)'").expect("valid"));

/// Fetch and parse one article.
pub async fn read(ctx: &Ctx, arg: &str) -> Result<Post> {
  let url = refs::article_url(arg)?;
  let resp = ctx
    .http
    .get(&url)
    .header(
      "accept",
      "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
    )
    .send()
    .await?;
  let html = resp.text();
  if resp.url.contains("wappoc_appmsgcaptcha") || html.contains("secitptpage/verify") {
    let hint = if url.contains("/s?") && !url.contains("chksm=") {
      "long article links need their `chksm` parameter; use the link as shared, or the short /s/... form"
    } else {
      "open the link in a browser once to pass the check, or retry later"
    };
    return Err(
      Error::new(
        ErrorCode::VerificationRequired,
        "WeChat answered with a captcha page (环境异常)",
      )
      .with_hint(hint),
    );
  }
  parse(&html, &url)
}

pub fn parse(html: &str, requested: &str) -> Result<Post> {
  let Some(content) = content(html) else {
    return Err(match MSG_TITLE.captures(html) {
      Some(m) => Error::not_found(format!("article unavailable: {}", entities(&m[1]))),
      None => Error::upstream("not an article page (no #js_content)"),
    });
  };
  let biz = var(html, "biz").or_else(|| var(html, "appuin"));
  let mid = var(html, "mid").or_else(|| var(html, "appmsgid"));
  let idx = var(html, "idx");
  let bizuin = biz.as_deref().and_then(refs::bizuin);
  let id = match (&bizuin, &mid, &idx) {
    (Some(b), Some(m), Some(i)) => format!("{b}_{m}_{i}"),
    _ => requested.rsplit('/').next().unwrap_or(requested).to_owned(),
  };
  let link = var(html, "msg_link")
    .map(|l| entities(&l))
    .and_then(|l| refs::article_url(&l).ok())
    .unwrap_or_else(|| requested.to_owned());
  let show_type = var(html, "item_show_type").unwrap_or_default();
  let kind = match show_type.as_str() {
    "5" => "video",
    "7" => "audio",
    "8" => "images",
    "10" => "text",
    _ => "article",
  };
  let title = text(html, "msg_title").or_else(|| {
    META_TITLE
      .captures(html)
      .map(|m| entities(&m[1]))
      .filter(|t| !t.is_empty())
  });
  let author = User {
    id: var(html, "user_name").unwrap_or_default(),
    name: text(html, "nickname").unwrap_or_default(),
    handle: var(html, "alias"),
    avatar: var(html, "round_head_img").or_else(|| var(html, "hd_head_img")),
    bio: text(html, "profile_signature"),
    extra: biz
      .iter()
      .map(|b| ("biz".to_owned(), Value::from(b.as_str())))
      .collect(),
    ..User::default()
  };

  let mut media = images(&content);
  if show_type == "8" {
    media.extend(pictures(html));
  }
  media.extend(videos(html));
  media.extend(voices(&content));

  let mut post = Post {
    id,
    kind: kind.into(),
    title,
    text: Some(html_to_text(&content)).filter(|t| !t.is_empty()),
    url: Some(link),
    author: Some(author),
    created_at: var(html, "ct")
      .or_else(|| var(html, "create_time"))
      .and_then(|t| t.parse().ok())
      .and_then(from_secs),
    media,
    raw: Some(json!({ "content_html": content })),
    ..Post::default()
  };
  let mut put = |key: &str, value: Option<Value>| {
    if let Some(v) = value {
      post.extra.insert(key.into(), v);
    }
  };
  let s = |v: Option<String>| v.map(Value::from);
  put("writer", s(text(html, "author")));
  put("digest", s(text(html, "msg_desc")));
  put("cover", s(var(html, "msg_cdn_url")));
  put("source_url", s(var(html, "msg_source_url")));
  put(
    "original",
    var(html, "copyright_stat").map(|c| Value::from(c == "1")),
  );
  put("category", s(text(html, "_ori_article_type")));
  put("ip_location", s(ip_location(html)));
  put(
    "comment_id",
    s(var(html, "comment_id").filter(|c| c != "0")),
  );
  put("biz", s(biz));
  put("mid", s(mid));
  put("idx", s(idx));
  put("sn", s(var(html, "sn")));
  let albums = albums(html);
  put("albums", (!albums.is_empty()).then(|| Value::from(albums)));
  Ok(post)
}

/// Inner HTML of `#js_content`: up to the `</div>` before the next script.
fn content(html: &str) -> Option<String> {
  let at = html.find("id=\"js_content\"")?;
  let start = at + html[at..].find('>')? + 1;
  let end = match html[start..].find("<script") {
    Some(s) => start + html[start..start + s].rfind("</div>")?,
    None => start + html[start..].rfind("</div>")?,
  };
  Some(html[start..end].trim().to_owned())
}

/// Content images at original size: `/0?wx_fmt=<fmt>`, tracking dropped.
fn images(content: &str) -> Vec<Media> {
  let mut seen = Vec::new();
  for m in IMAGE.captures_iter(content) {
    if let Some(url) = original(&entities(&m[1]))
      && !seen.contains(&url)
    {
      seen.push(url);
    }
  }
  seen
    .into_iter()
    .map(|url| {
      let gif = url.contains("wx_fmt=gif");
      Media::new(
        if gif {
          MediaKind::Gif
        } else {
          MediaKind::Image
        },
        url,
      )
    })
    .collect()
}

pub fn original(url: &str) -> Option<String> {
  let mut u = Url::parse(url).ok()?;
  let _ = u.set_scheme("https");
  let fmt = u
    .query_pairs()
    .find(|(k, _)| k == "wx_fmt")
    .map(|(_, v)| v.into_owned());
  let mut segments: Vec<String> = u.path_segments()?.map(str::to_owned).collect();
  if segments
    .last()
    .is_some_and(|s| s.bytes().all(|b| b.is_ascii_digit()))
  {
    segments.pop();
  }
  segments.push("0".into());
  u.set_path(&segments.join("/"));
  match fmt {
    Some(f) => u.set_query(Some(&format!("wx_fmt={f}"))),
    None => u.set_query(None),
  }
  Some(u.into())
}

/// Images of a 图片消息 (`item_show_type` 8), listed in the page data.
fn pictures(html: &str) -> Vec<Media> {
  let Some(at) = html.find("picture_page_info_list") else {
    return Vec::new();
  };
  let block = &html[at..];
  let block = &block[..block.find("];").unwrap_or(block.len())];
  let mut seen: Vec<String> = Vec::new();
  for m in PICTURE.captures_iter(block) {
    if let Some(url) = original(&m[1])
      && !seen.contains(&url)
    {
      seen.push(url);
    }
  }
  seen.into_iter().map(Media::image).collect()
}

/// Each article video at its largest quality. The URLs keep their literal
/// `&amp;`: the video host refuses them decoded (and wants the mp Referer).
fn videos(html: &str) -> Vec<Media> {
  let Some(at) = html.find("var videoPageInfos") else {
    return Vec::new();
  };
  let block = &html[at..];
  let block = &block[..block.find("];").unwrap_or(block.len())];
  let mut out = Vec::new();
  for video in block.split("video_id:").skip(1) {
    let best = QUALITY
      .captures_iter(video)
      .filter_map(|m| {
        let size: u64 = m[1].parse().ok()?;
        let url = m[3]
          .replace("\\x26", "&")
          .replacen("http://", "https://", 1);
        Some((size, m[2].parse().ok(), m[4].parse().ok(), url))
      })
      .max_by_key(|(size, ..)| *size);
    if let Some((size, height, width, url)) = best {
      let mut m = Media::video(url);
      m.size = Some(size);
      m.width = width;
      m.height = height;
      m.duration = DURATION
        .captures(video)
        .and_then(|d| d[1].parse::<f64>().ok())
        .map(|ms| ms / 1000.0);
      out.push(m);
    }
  }
  out
}

fn voices(content: &str) -> Vec<Media> {
  VOICE
    .captures_iter(content)
    .map(|m| {
      Media::new(
        MediaKind::Audio,
        format!("https://res.wx.qq.com/voice/getvoice?mediaid={}", &m[1]),
      )
    })
    .collect()
}

fn ip_location(html: &str) -> Option<String> {
  let at = html.find("window.ip_wording = {")?;
  let block = &html[at..];
  let block = &block[..block.find('}')?];
  let field = |name: &str| {
    let at = block.find(name)?;
    literals(&block[at + name.len()..])
      .into_iter()
      .next()
      .filter(|v| !v.is_empty())
  };
  field("provinceName").or_else(|| field("countryName"))
}

fn albums(html: &str) -> Vec<Value> {
  let Some(at) = html.find("var album_info_list") else {
    return Vec::new();
  };
  let block = &html[at..];
  let block = &block[..block.find("];").unwrap_or(block.len())];
  let biz = var(html, "biz").unwrap_or_default();
  ALBUM
    .captures_iter(block)
    .map(|m| {
      json!({
        "id": &m[2],
        "title": entities(&m[1]),
        "url": refs::album_url(&biz, &m[2]),
      })
    })
    .collect()
}

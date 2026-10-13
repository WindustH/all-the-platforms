//! Without an account: the public web preview of channels
//! (`t.me/s/<name>`, 20 posts a page, older ones with `?before=<id>`, a
//! search with `?q=`), single posts (`t.me/<name>/<id>?embed=1`) and
//! profile pages (`t.me/<name>`). Groups and private chats have no preview.

use std::sync::LazyLock;

use regex::Regex;
use social_core::chat;
use social_core::text::{html_to_text, parse_count};
use social_core::{Ctx, Error, Media, MediaKind, Page, Post, Result, User, Value, json};

const HOME: &str = "https://t.me";

macro_rules! re {
  ($name:ident, $pattern:literal) => {
    static $name: LazyLock<Regex> = LazyLock::new(|| Regex::new($pattern).unwrap());
  };
}

re!(POST, r#"^([A-Za-z0-9_]+)/(\d+)""#);
re!(
  OWNER,
  r#"(?s)tgme_widget_message_owner_name" href="([^"]*)"><span dir="auto">(.*?)</span>"#
);
re!(
  SIGNATURE,
  r#"(?s)tgme_widget_message_from_author" dir="auto">(.*?)</span>"#
);
re!(
  TEXT,
  r#"(?s)<div class="tgme_widget_message_text js-message_text"[^>]*>(.*?)</div>"#
);
re!(VIEWS, r#"tgme_widget_message_views">([^<]+)<"#);
re!(TIME, r#"<time datetime="([^"]+)""#);
re!(
  PHOTO,
  r#"tgme_widget_message_photo_wrap[^"]*"[^>]*?background-image:url\('([^']+)'\)"#
);
re!(VIDEO, r#"<video src="([^"]+)""#);
re!(AUDIO, r#"<audio src="([^"]+)""#);
re!(DURATION, r#"message_video_duration[^>]*>([0-9:]+)<"#);
re!(
  DOCUMENT,
  r#"(?s)tgme_widget_message_document_title[^>]*>(.*?)</div>"#
);
re!(
  PREVIEW,
  r#"tgme_widget_message_link_preview" href="([^"]+)""#
);
re!(
  FORWARD,
  r#"(?s)tgme_widget_message_forwarded_from_name"(?: href="([^"]*)")?><span dir="auto">(.*?)</span>"#
);
re!(REPLY, r#"tgme_widget_message_reply" href="([^"]+)""#);
re!(
  REACTION,
  r#"(?s)<span class="tgme_reaction( tgme_reaction_paid)?">(.*?)</span>"#
);
re!(EMOJI, r#"<b>([^<]+)</b>"#);
re!(EMOJI_ID, r#"emoji-id="(\d+)""#);
re!(TRAILING_COUNT, r#"([0-9][0-9.,]*[KMB]?)\s*$"#);
re!(
  POLL,
  r#"(?s)tgme_widget_message_poll_question">(.*?)</div>"#
);
re!(MORE, r#"tme_messages_more[^"]*" data-before="(\d+)""#);
re!(
  CHANNEL_TITLE,
  r#"(?s)tgme_channel_info_header_title"><span dir="auto">(.*?)</span>"#
);
re!(
  CHANNEL_DESC,
  r#"(?s)tgme_channel_info_description">(.*?)</div>"#
);
re!(
  CHANNEL_PHOTO,
  r#"(?s)tgme_channel_info_header">\s*<i class="tgme_page_photo_image[^"]*"[^>]*><img src="([^"]+)""#
);
re!(
  COUNTER,
  r#"counter_value">([^<]+)</span> <span class="counter_type">([^<]+)<"#
);
re!(
  PAGE_TITLE,
  r#"(?s)tgme_page_title"[^>]*>\s*<span dir="auto">(.*?)</span>"#
);
re!(PAGE_EXTRA, r#"(?s)tgme_page_extra">(.*?)</div>"#);
re!(PAGE_DESC, r#"(?s)tgme_page_description"[^>]*>(.*?)</div>"#);
re!(PAGE_PHOTO, r#"tgme_page_photo_image" src="([^"]+)""#);
re!(PREVIEW_LINK, r#"tgme_page_context_link" href="/s/"#);

async fn page(ctx: &Ctx, url: &str) -> Result<String> {
  let resp = ctx
    .http
    .get(url)
    .no_cookies()
    .header("accept-language", "en-US,en;q=0.9")
    .send()
    .await?
    .check()?;
  Ok(resp.text())
}

/// A page of a channel's posts, newest first; the cursor is the oldest id.
pub async fn posts(
  ctx: &Ctx,
  name: &str,
  query: Option<&str>,
  before: Option<&str>,
) -> Result<Page<Post>> {
  let mut url = format!("{HOME}/s/{name}");
  let mut params = Vec::new();
  if let Some(q) = query {
    params.push(format!(
      "q={}",
      url::form_urlencoded::byte_serialize(q.as_bytes()).collect::<String>()
    ));
  }
  if let Some(b) = before {
    params.push(format!("before={b}"));
  }
  if !params.is_empty() {
    url = format!("{url}?{}", params.join("&"));
  }
  let html = page(ctx, &url).await?;
  if !html.contains("tgme_channel_history") {
    return Err(
      Error::not_found(format!(
        "@{name} has no public web preview (only public channels have one)"
      ))
      .with_hint("log in to read groups, private chats and channels without a preview"),
    );
  }
  let chat_name = CHANNEL_TITLE.captures(&html).map(|c| html_to_text(&c[1]));
  let mut items = messages(&html, chat_name.as_deref());
  items.reverse();
  let next = MORE.captures(&html).map(|c| c[1].to_owned());
  // The oldest post links to nothing older.
  let next = next.filter(|b| b.parse::<i64>().is_ok_and(|b| b > 1) && !items.is_empty());
  Ok(Page::new(items, next))
}

/// One public post.
pub async fn post(ctx: &Ctx, name: &str, id: i32) -> Result<Post> {
  let html = page(ctx, &format!("{HOME}/{name}/{id}?embed=1&mode=tme")).await?;
  messages(&html, None)
    .into_iter()
    .next()
    .ok_or_else(|| Error::not_found(format!("no public post t.me/{name}/{id}")))
}

/// A public profile: a channel's preview header when it has one (with its
/// counters), otherwise the `t.me/<name>` page.
pub async fn profile(ctx: &Ctx, name: &str) -> Result<User> {
  let html = page(ctx, &format!("{HOME}/{name}")).await?;
  let Some(title) = PAGE_TITLE.captures(&html).map(|c| html_to_text(&c[1])) else {
    return Err(Error::not_found(format!(
      "no Telegram user, group or channel @{name}"
    )));
  };
  let extra_line = PAGE_EXTRA
    .captures(&html)
    .map(|c| html_to_text(&c[1]))
    .unwrap_or_default();
  let mut user = User {
    id: name.to_owned(),
    name: title,
    handle: Some(name.to_owned()),
    url: Some(format!("{HOME}/{name}")),
    avatar: PAGE_PHOTO.captures(&html).map(|c| c[1].to_owned()),
    bio: PAGE_DESC
      .captures(&html)
      .map(|c| html_to_text(&c[1]))
      .filter(|d| !d.is_empty()),
    verified: html.contains("verified-icon"),
    ..User::default()
  };
  let kind = kind_and_counts(&mut user, &extra_line);
  user.extra.insert("kind".into(), kind.into());
  if PREVIEW_LINK.is_match(&html) {
    preview_counters(ctx, name, &mut user).await;
  }
  Ok(user)
}

/// What the header line says ("10 405 871 subscribers", "1 234 members, 56
/// online" or "@name"): the kind of chat, its counters into `user`.
fn kind_and_counts(user: &mut User, line: &str) -> &'static str {
  let count = |word: &str| {
    line.split(',').find_map(|part| {
      let part = part.trim();
      let number = part.strip_suffix(word)?.trim().replace([' ', '\u{a0}'], "");
      number.parse::<u64>().ok()
    })
  };
  if let Some(n) = count("subscribers").or_else(|| count("subscriber")) {
    user.stats.followers = Some(n);
    "channel"
  } else if let Some(n) = count("members").or_else(|| count("member")) {
    user.stats.followers = Some(n);
    if let Some(online) = count("online") {
      user.stats.other.insert("online".into(), online);
    }
    "group"
  } else if line.starts_with('@') {
    "user"
  } else {
    "unknown"
  }
}

/// A channel's preview adds media counters, and its description and photo
/// when the profile page lacks them.
async fn preview_counters(ctx: &Ctx, name: &str, user: &mut User) {
  let Ok(preview) = page(ctx, &format!("{HOME}/s/{name}")).await else {
    return;
  };
  for c in COUNTER.captures_iter(&preview) {
    if let Some(n) = parse_count(&c[1]) {
      let key = c[2].trim().to_owned();
      if key.starts_with("subscriber") {
        user.stats.followers = Some(n);
      } else {
        user.stats.other.insert(key, n);
      }
    }
  }
  if user.bio.is_none() {
    user.bio = CHANNEL_DESC.captures(&preview).map(|c| html_to_text(&c[1]));
  }
  if user.avatar.is_none() {
    user.avatar = CHANNEL_PHOTO.captures(&preview).map(|c| c[1].to_owned());
  }
}

/// Every message of a page, in page order (oldest first).
fn messages(html: &str, chat_name: Option<&str>) -> Vec<Post> {
  html
    .split("data-post=\"")
    .skip(1)
    .filter_map(|chunk| message(chunk, chat_name))
    .collect()
}

fn message(chunk: &str, chat_name: Option<&str>) -> Option<Post> {
  let caps = POST.captures(chunk)?;
  let (name, id) = (caps[1].to_owned(), caps[2].parse::<i32>().ok()?);
  let owner = OWNER.captures(chunk);
  let owner_name = owner.as_ref().map(|c| html_to_text(&c[2]));
  let mut post = Post {
    id: chat::message_id(&name, &id.to_string()),
    kind: if chunk.contains("service_message") {
      "service".into()
    } else {
      "message".into()
    },
    text: TEXT
      .captures(chunk)
      .map(|c| html_to_text(&c[1]))
      .filter(|t| !t.is_empty()),
    url: Some(format!("{HOME}/{name}/{id}")),
    author: Some(User {
      id: name.clone(),
      name: owner_name.clone().unwrap_or_else(|| name.clone()),
      handle: Some(name.clone()),
      url: Some(format!("{HOME}/{name}")),
      ..User::default()
    }),
    created_at: TIME.captures(chunk).and_then(|c| c[1].parse().ok()),
    ..Post::default()
  };
  post.metrics.views = VIEWS.captures(chunk).and_then(|c| parse_count(&c[1]));
  let (reactions, total, paid) = reactions(chunk);
  if total > 0 {
    post.metrics.likes = Some(total);
  }
  if paid > 0 {
    post.metrics.other.insert("stars".into(), paid);
  }
  post.media = media(chunk);
  let extra = &mut post.extra;
  extra.insert("chat".into(), name.clone().into());
  if let Some(n) = chat_name.map(str::to_owned).or(owner_name) {
    extra.insert("chat_name".into(), n.into());
  }
  extra.insert("message_id".into(), id.into());
  if !reactions.is_empty() {
    extra.insert("reactions".into(), Value::Array(reactions));
  }
  message_extra(&mut post, chunk);
  Some(post)
}

/// Signature, edit mark, forward, reply, link preview, files and polls.
fn message_extra(post: &mut Post, chunk: &str) {
  let extra = &mut post.extra;
  if let Some(s) = SIGNATURE.captures(chunk) {
    extra.insert("signature".into(), html_to_text(&s[1]).into());
  }
  if chunk.contains("edited &nbsp;") {
    extra.insert("edited".into(), true.into());
  }
  if let Some(f) = FORWARD.captures(chunk) {
    extra.insert(
      "forwarded_from".into(),
      json!({"name": html_to_text(&f[2]), "url": f.get(1).map(|u| u.as_str())}),
    );
  }
  if let Some(r) = REPLY.captures(chunk) {
    extra.insert("reply_to".into(), r[1].into());
  }
  if let Some(l) = PREVIEW.captures(chunk) {
    extra.insert("link".into(), l[1].into());
  }
  let files: Vec<Value> = DOCUMENT
    .captures_iter(chunk)
    .map(|d| html_to_text(&d[1]).into())
    .collect();
  if !files.is_empty() {
    extra.insert("files".into(), Value::Array(files));
  }
  if let Some(p) = POLL.captures(chunk) {
    extra.insert("poll".into(), html_to_text(&p[1]).into());
  }
}

/// Reactions as `{emoji, count}` (custom emoji by id), their total, and paid stars.
fn reactions(chunk: &str) -> (Vec<Value>, u64, u64) {
  let (mut list, mut total, mut paid) = (Vec::new(), 0, 0);
  for r in REACTION.captures_iter(chunk) {
    let body = &r[2];
    let Some(count) = TRAILING_COUNT
      .captures(body)
      .and_then(|c| parse_count(&c[1]))
    else {
      continue;
    };
    if r.get(1).is_some() {
      paid += count;
      continue;
    }
    total += count;
    let emoji = EMOJI
      .captures(body)
      .map(|e| e[1].to_owned())
      .or_else(|| EMOJI_ID.captures(body).map(|e| format!("custom:{}", &e[1])))
      .unwrap_or_default();
    list.push(json!({"emoji": emoji, "count": count}));
  }
  (list, total, paid)
}

fn media(chunk: &str) -> Vec<Media> {
  let mut out: Vec<Media> = PHOTO
    .captures_iter(chunk)
    .map(|c| Media::image(&c[1]))
    .collect();
  let durations: Vec<f64> = DURATION
    .captures_iter(chunk)
    .filter_map(|c| seconds(&c[1]))
    .collect();
  for (i, v) in VIDEO.captures_iter(chunk).enumerate() {
    let mut m = Media::video(&v[1]);
    m.duration = durations.get(i).copied();
    out.push(m);
  }
  for a in AUDIO.captures_iter(chunk) {
    out.push(Media::new(MediaKind::Audio, &a[1]));
  }
  out
}

/// `1:02:05` -> 3725.
fn seconds(text: &str) -> Option<f64> {
  text.split(':').try_fold(0.0, |acc, part| {
    part.parse::<f64>().ok().map(|n| acc * 60.0 + n)
  })
}

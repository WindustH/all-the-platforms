//! Single items as cards: a post, a user, a comment thread, actions, the
//! lines of `listen`.

use super::*;

/// One event of `listen`: time, chat, author and text on a line.
pub fn stream_line(p: &Post) {
  let time = p
    .created_at
    .map(|t| {
      t.to_zoned(jiff::tz::TimeZone::system())
        .strftime("%H:%M:%S")
        .to_string()
    })
    .unwrap_or_default();
  let chat = p
    .extra
    .get("chat_name")
    .or_else(|| p.extra.get("chat"))
    .and_then(|v| v.as_str())
    .unwrap_or_default();
  let author = p.author.as_ref().map(user_label).unwrap_or_default();
  let kind = match p.kind.as_str() {
    "" | "message" => String::new(),
    k => format!("[{k}] "),
  };
  let mut line = format!("{time}  {chat}  {author}: {kind}{}", headline(p));
  if !p.media.is_empty() {
    line += &format!("  (+{} files)", p.media.len());
  }
  println!("{}", line.trim_start());
}

pub(super) fn comment_tree(comments: &[Comment]) {
  for (i, c) in comments.iter().enumerate() {
    comment(c, 0, Some(i + 1));
  }
}

pub(super) fn comment(c: &Comment, depth: usize, index: Option<usize>) {
  let pad = "  ".repeat(depth);
  let mut meta = Vec::new();
  if let Some(t) = c.created_at {
    meta.push(fmt_time(t));
  }
  if let Some(l) = c.likes.filter(|l| *l > 0) {
    meta.push(format!("{} likes", fmt_count(l)));
  }
  if let Some(r) = c.reply_count.filter(|r| *r > 0) {
    meta.push(format!("{} replies", fmt_count(r)));
  }
  if let Some(loc) = &c.location {
    meta.push(loc.clone());
  }
  meta.push(format!("id {}", c.id));
  let name = c.author.as_ref().map(|a| a.name.as_str()).unwrap_or("?");
  let name = match &c.reply_to {
    Some(to) => format!("{name} → {to}"),
    None => name.to_owned(),
  };
  let marker = match index {
    Some(i) => format!("{i}."),
    None => "↳".to_owned(),
  };
  println!(
    "{pad}{} {} {}",
    marker.if_supports_color(Stream::Stdout, |t| t.dimmed()),
    name.if_supports_color(Stream::Stdout, |t| t.bold()),
    format!("· {}", meta.join(" · ")).if_supports_color(Stream::Stdout, |t| t.dimmed())
  );
  for line in c.text.lines() {
    println!("{pad}   {line}");
  }
  for r in &c.replies {
    comment(r, depth + 1, None);
  }
}

pub(super) fn metrics_line(m: &Metrics) -> String {
  let mut parts = Vec::new();
  for (label, v) in [
    ("views", m.views),
    ("likes", m.likes),
    ("comments", m.comments),
    ("shares", m.shares),
    ("favorites", m.favorites),
  ] {
    if let Some(v) = v {
      parts.push(format!("{} {label}", fmt_count(v)));
    }
  }
  for (k, v) in &m.other {
    parts.push(format!("{} {k}", fmt_count(*v)));
  }
  parts.join(" · ")
}

pub(super) fn post_card(p: &Post) {
  if let Some(title) = &p.title {
    println!("{}", title.if_supports_color(Stream::Stdout, |t| t.bold()));
  }
  let mut meta = vec![p.kind.clone()];
  if let Some(a) = &p.author {
    meta.push(user_label(a));
  }
  if let Some(t) = p.created_at {
    meta.push(fmt_time(t));
  }
  meta.push(format!("id {}", p.id));
  println!(
    "{}",
    meta
      .join(" · ")
      .if_supports_color(Stream::Stdout, |t| t.dimmed())
  );
  let metrics = metrics_line(&p.metrics);
  if !metrics.is_empty() {
    println!("{metrics}");
  }
  if let Some(price) = &p.price {
    price_line(price);
  }
  if let Some(url) = &p.url {
    println!("{}", url.if_supports_color(Stream::Stdout, |t| t.cyan()));
  }
  if let Some(text) = p.text.as_deref().filter(|t| !t.is_empty()) {
    println!("\n{text}");
  }
  if !p.tags.is_empty() {
    println!(
      "\n{}",
      p.tags
        .iter()
        .map(|t| format!("#{t}"))
        .collect::<Vec<_>>()
        .join(" ")
    );
  }
  if !p.media.is_empty() {
    media_lines(p);
  }
  if let Some(q) = &p.quoted {
    quoted(q);
  }
}

fn price_line(price: &Price) {
  let mut line = price.display();
  if let Some(original) = price.original {
    let before = Price {
      amount: original,
      ..price.clone()
    };
    line.push_str(&format!(" (was {})", before.display()));
  }
  if let Some(label) = &price.label {
    line.push_str(&format!(" · {label}"));
  }
  println!("{}", line.if_supports_color(Stream::Stdout, |t| t.bold()));
}

fn media_lines(p: &Post) {
  println!();
  for m in &p.media {
    let dur = m
      .duration
      .map(|d| format!(" ({})", fmt_duration(d)))
      .unwrap_or_default();
    println!(
      "{}{dur} {}",
      format!("{:?}", m.kind).to_lowercase(),
      m.url.if_supports_color(Stream::Stdout, |t| t.dimmed())
    );
  }
}

fn quoted(q: &Post) {
  println!(
    "\n{}",
    "┃ quoted".if_supports_color(Stream::Stdout, |t| t.dimmed())
  );
  let who = q.author.as_ref().map(user_label).unwrap_or_default();
  println!("┃ {who}");
  for line in headline(q).lines() {
    println!("┃ {line}");
  }
}

pub(super) fn user_card(u: &User) {
  println!(
    "{}",
    user_label(u).if_supports_color(Stream::Stdout, |t| t.bold())
  );
  let mut meta = vec![format!("id {}", u.id)];
  if u.verified {
    meta.push("verified".into());
  }
  if let Some(l) = &u.location {
    meta.push(l.clone());
  }
  if let Some(true) = u.followed {
    meta.push("following".into());
  }
  println!(
    "{}",
    meta
      .join(" · ")
      .if_supports_color(Stream::Stdout, |t| t.dimmed())
  );
  let s = &u.stats;
  let mut stats = Vec::new();
  for (label, v) in [
    ("followers", s.followers),
    ("following", s.following),
    ("posts", s.posts),
    ("likes", s.likes),
  ] {
    if let Some(v) = v {
      stats.push(format!("{} {label}", fmt_count(v)));
    }
  }
  for (k, v) in &s.other {
    stats.push(format!("{} {k}", fmt_count(*v)));
  }
  if !stats.is_empty() {
    println!("{}", stats.join(" · "));
  }
  if let Some(url) = &u.url {
    println!("{}", url.if_supports_color(Stream::Stdout, |t| t.cyan()));
  }
  if let Some(bio) = u.bio.as_deref().filter(|b| !b.is_empty()) {
    println!("\n{bio}");
  }
}

pub(super) fn action_line(a: &Action) {
  let mark = if a.ok {
    "✓"
      .if_supports_color(Stream::Stdout, |t| t.green())
      .to_string()
  } else {
    "✗".to_owned()
  };
  let mut line = format!("{mark} {} {}", a.action, a.target);
  if let Some(id) = &a.id {
    line.push_str(&format!(" → {id}"));
  }
  println!("{line}");
  if let Some(url) = &a.url {
    println!("{}", url.if_supports_color(Stream::Stdout, |t| t.cyan()));
  }
  if let Some(m) = &a.message {
    note(m);
  }
}

pub(super) fn auth_line(a: &AuthStatus) {
  match (&a.user, a.authenticated) {
    (Some(u), true) => {
      let source = a
        .source
        .as_deref()
        .map(|s| format!(" (via {s})"))
        .unwrap_or_default();
      println!(
        "{} logged in as {} · id {}{source}",
        "✓".if_supports_color(Stream::Stdout, |t| t.green()),
        user_label(u),
        u.id
      );
    }
    _ => println!(
      "{} not logged in",
      "✗".if_supports_color(Stream::Stdout, |t| t.red())
    ),
  }
  if let Some(m) = &a.message {
    note(m);
  }
}

pub(super) fn transcript(t: &Transcript) {
  note(&format!("language: {}", t.lang));
  for c in &t.cues {
    println!(
      "{} {}",
      format!("[{}]", fmt_duration(c.from)).if_supports_color(Stream::Stdout, |t| t.dimmed()),
      c.text
    );
  }
}

pub(super) fn downloads(files: &[Downloaded]) {
  for f in files {
    println!(
      "{} {} ({})",
      format!("{:?}", f.kind).to_lowercase(),
      f.path,
      fmt_count(f.bytes) + "B"
    );
  }
}

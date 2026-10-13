//! Hot lists: hot-search entries and topics.

use super::*;

/// A hot-list entry (热搜) as a post of kind `trend`; `None` for promotions.
pub fn trend(v: &Value, rank: usize) -> Option<Post> {
  let word = v.first_str(&["word", "note"])?;
  if v.i64("is_ad") == Some(1) || v.str("promotion").is_some() {
    return None;
  }
  let query = v
    .str("word_scheme")
    .filter(|w| w.starts_with('#'))
    .unwrap_or_else(|| word.clone());
  let url = format!(
    "https://s.weibo.com/weibo?q={}",
    url::form_urlencoded::byte_serialize(query.as_bytes()).collect::<String>()
  );
  let mut post = Post {
    id: v
      .first_str(&["mid", "word"])
      .unwrap_or_else(|| word.clone()),
    kind: "trend".into(),
    title: Some(word.clone()),
    text: v.str("note").filter(|n| n != &word),
    url: Some(url),
    created_at: v.i64("onboard_time").and_then(atp_core::text::from_secs),
    metrics: Metrics {
      views: v.first_count(&["num", "raw_hot"]),
      ..Metrics::default()
    },
    tags: v.str("category").into_iter().collect(),
    raw: Some(v.clone()),
    ..Post::default()
  };
  let extra = &mut post.extra;
  extra.insert(
    "rank".into(),
    v.i64("realpos").unwrap_or(rank as i64 + 1).into(),
  );
  if let Some(label) = v
    .first_str(&["label_name", "icon_desc"])
    .filter(|l| !l.is_empty())
  {
    extra.insert("label".into(), label.into());
  }
  Some(post)
}

/// A topic (话题) search result card.
pub fn topic(v: &Value) -> Option<Collection> {
  let title = v.str("title_sub")?;
  let name = title.trim_matches('#').to_owned();
  let stats = v.str("desc2").unwrap_or_default();
  let mut topic = Collection {
    id: name.clone(),
    kind: "topic".into(),
    description: v.str("desc1").filter(|d| !d.is_empty()),
    url: Some(format!(
      "https://s.weibo.com/weibo?q={}",
      url::form_urlencoded::byte_serialize(format!("#{name}#").as_bytes()).collect::<String>()
    )),
    raw: Some(v.clone()),
    name,
    ..Collection::default()
  };
  // "324.5万讨论 48.2亿阅读"
  for part in stats.split_whitespace() {
    let (num, label) = part.split_at(
      part
        .char_indices()
        .find(|(_, ch)| !(ch.is_ascii_digit() || *ch == '.' || *ch == '万' || *ch == '亿'))
        .map_or(part.len(), |(i, _)| i),
    );
    let count = atp_core::text::parse_count(num);
    match label {
      "讨论" => topic.items = count,
      "阅读" => topic.views = count,
      _ => {}
    }
  }
  Some(topic)
}

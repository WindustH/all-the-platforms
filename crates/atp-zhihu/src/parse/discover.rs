//! Columns, trending searches and Zhihu Daily (日报) stories.

use atp_core::text::html_to_text;
use atp_core::{Collection, Media, Post, User, Value, ValueExt};
use jiff::civil::{Date, Time};
use jiff::tz::{Offset, TimeZone};

use super::{time, user};

/// A column (专栏).
pub fn column(v: &Value) -> Collection {
  let id = v.str("id").unwrap_or_default();
  let mut c = Collection {
    url: Some(format!("https://zhuanlan.zhihu.com/{id}")),
    id,
    kind: "column".into(),
    name: v.str("title").unwrap_or_default(),
    description: v
      .first_str(&["intro", "description"])
      .map(|d| html_to_text(&d))
      .filter(|d| !d.is_empty()),
    items: v.first_count(&["articles_count", "items_count"]),
    followers: v.first_count(&["followers", "follower_count"]),
    owner: v.at("author").is_object().then(|| user(v.at("author"))),
    raw: Some(v.clone()),
    ..Collection::default()
  };
  if let Some(at) = time(v, &["updated"]) {
    c.extra.insert("updated_at".into(), at.to_string().into());
  }
  c
}

/// A trending search (热搜) as a query to search for.
pub fn query(v: &Value) -> Option<Collection> {
  let name = v.first_str(&["query", "real_query"])?;
  let mut c = Collection {
    id: v.str("query_id").unwrap_or_else(|| name.clone()),
    url: url::Url::parse_with_params(
      "https://www.zhihu.com/search",
      [("type", "content"), ("q", name.as_str())],
    )
    .ok()
    .map(String::from),
    kind: "query".into(),
    description: v.str("hot_show").map(|h| format!("{h}热度")),
    name,
    raw: Some(v.clone()),
    ..Collection::default()
  };
  if let Some(n) = v.u64("hot") {
    c.extra.insert("heat".into(), n.into());
  }
  if let Some(label) = v.str("label") {
    c.extra.insert("label".into(), label.into());
  }
  Some(c)
}

/// A Zhihu Daily story; `hint` reads `作者 / 名字` or `名字 · 2 分钟阅读`.
pub fn daily_story(v: &Value, date: Option<&str>) -> Option<Post> {
  let id = v.str("id")?;
  let hint = v.str("hint").unwrap_or_default();
  let author = hint
    .strip_prefix("作者 / ")
    .or_else(|| hint.split(" · ").next())
    .map(str::trim)
    .filter(|a| !a.is_empty());
  let mut p = Post {
    url: Some(
      v.str("url")
        .unwrap_or_else(|| format!("https://daily.zhihu.com/story/{id}")),
    ),
    id,
    kind: "daily".into(),
    title: v.str("title"),
    author: author.map(|name| User {
      name: name.to_owned(),
      ..User::default()
    }),
    media: v
      .list("images")
      .iter()
      .filter_map(Value::as_str)
      .chain(v.str("image").as_deref())
      .map(Media::image)
      .collect(),
    raw: Some(v.clone()),
    ..Post::default()
  };
  if let Some(d) = date {
    p.created_at = day_start(d);
    p.extra.insert("date".into(), d.into());
  }
  if !hint.is_empty() {
    p.extra.insert("hint".into(), hint.into());
  }
  Some(p)
}

/// Midnight in Beijing of a `YYYYMMDD` day: when Daily publishes it.
fn day_start(day: &str) -> Option<jiff::Timestamp> {
  let date = Date::strptime("%Y%m%d", day).ok()?;
  let beijing = TimeZone::fixed(Offset::constant(8));
  date
    .to_datetime(Time::midnight())
    .to_zoned(beijing)
    .ok()
    .map(|z| z.timestamp())
}

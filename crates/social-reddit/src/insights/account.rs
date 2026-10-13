//! The logged-in account: native totals (karma, followers, age, trophies,
//! karma per community) and trends computed from its own posts and comments.

use std::collections::BTreeMap;

use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::TimeZone;
use social_core::{ErrorCode, Insights, PageReq, Point, Result, Series, Value, ValueExt, json};

use super::{breakdown, day};
use crate::api::Api;
use crate::listings;
use crate::refs::user_url;

/// Pages of 100 own posts / comments read for the trends.
const MAX_PAGES: usize = 10;

/// The last `days` days, oldest first.
struct Window {
  dates: Vec<Date>,
  /// Unix seconds of the first day's start.
  since: i64,
  tz: TimeZone,
}

impl Window {
  fn last(days: u32) -> Self {
    let tz = TimeZone::system();
    let today = Timestamp::now().to_zoned(tz.clone()).date();
    let dates: Vec<Date> = (0..days as i64)
      .rev()
      .filter_map(|i| today.checked_sub(jiff::Span::new().days(i)).ok())
      .collect();
    let since = dates
      .first()
      .and_then(|d| d.to_zoned(tz.clone()).ok())
      .map_or(0, |z| z.timestamp().as_second());
    Self { dates, since, tz }
  }
}

/// The logged-in account over the last `days` days.
pub async fn account(api: &Api, days: u32) -> Result<Insights> {
  let me = api.me().await?;
  let name = me.str("name").unwrap_or_default();
  let window = Window::last(days);
  let mut ins = Insights {
    kind: "account".into(),
    subject: name.clone(),
    title: Some(format!("u/{name}")),
    url: Some(user_url(&name)),
    from: window.dates.first().map(ToString::to_string),
    to: window.dates.last().map(ToString::to_string),
    ..Insights::default()
  };
  native_totals(&me, &mut ins);
  let mut missing = Vec::new();
  let karma = karma(api, &mut ins, &mut missing).await?;
  let trophies = trophies(api, &window.tz, &mut ins, &mut missing).await?;
  computed(api, &name, &window, &mut ins).await?;
  describe(&mut ins);
  ins.warnings.extend(missing);
  ins.raw = Some(json!({ "me": me, "karma": karma, "trophies": trophies }));
  Ok(ins)
}

/// Karma, followers and account age from the account itself.
fn native_totals(me: &Value, ins: &mut Insights) {
  ins.total("karma", me.i64("total_karma").map(Value::from));
  ins.total("post_karma", me.i64("link_karma").map(Value::from));
  ins.total("comment_karma", me.i64("comment_karma").map(Value::from));
  ins.total("awarder_karma", me.i64("awarder_karma").map(Value::from));
  ins.total("awardee_karma", me.i64("awardee_karma").map(Value::from));
  ins.total(
    "followers",
    me.u64("subreddit.subscribers").map(Value::from),
  );
  let age = me
    .i64("created_utc")
    .map(|c| (Timestamp::now().as_second() - c) / 86_400);
  ins.total("account_age_days", age.map(Value::from));
}

/// Karma per community, as breakdowns.
async fn karma(api: &Api, ins: &mut Insights, missing: &mut Vec<String>) -> Result<Value> {
  let karma = optional(
    api.get_oauth("/api/v1/me/karma", &[]).await,
    "karma",
    missing,
  )?;
  for (dimension, keys) in [
    ("karma_by_subreddit", &["link_karma", "comment_karma"][..]),
    ("post_karma_by_subreddit", &["link_karma"][..]),
    ("comment_karma_by_subreddit", &["comment_karma"][..]),
  ] {
    let counts = karma
      .list("data")
      .iter()
      .filter_map(|k| {
        let sum = keys.iter().map(|f| k.i64(f).unwrap_or(0)).sum();
        Some((k.str("sr")?, sum))
      })
      .collect();
    ins.breakdowns.extend(breakdown(dimension, counts));
  }
  Ok(karma)
}

/// Trophies with the day each was granted.
async fn trophies(
  api: &Api,
  tz: &TimeZone,
  ins: &mut Insights,
  missing: &mut Vec<String>,
) -> Result<Value> {
  let trophies = optional(
    api.get_oauth("/api/v1/me/trophies", &[]).await,
    "trophies",
    missing,
  )?;
  let list: Vec<Value> = trophies
    .list("data.trophies")
    .iter()
    .map(|t| t.at("data"))
    .map(|t| {
      let granted = t.i64("granted_at").and_then(|s| day(s, tz));
      json!({ "name": t.str("name"), "granted_at": granted.map(|d| d.to_string()),
        "description": t.str("description") })
    })
    .collect();
  if !trophies.is_null() {
    ins.total("trophies", Some(Value::from(list.len())));
    ins.extra.insert("trophies".into(), list.into());
  }
  Ok(trophies)
}

/// Daily series and totals of the account's own posts and comments created
/// in the window, their mean upvote ratio and where they were posted.
async fn computed(api: &Api, name: &str, window: &Window, ins: &mut Insights) -> Result<()> {
  let posts = own(api, name, "submitted", window.since).await?;
  let comments = own(api, name, "comments", window.since).await?;
  let metrics = [
    ("posts", &posts, None),
    ("likes", &posts, Some("score")),
    ("comments", &posts, Some("num_comments")),
    ("crossposts", &posts, Some("num_crossposts")),
    ("comments_written", &comments, None),
    ("comment_likes", &comments, Some("score")),
  ];
  for (metric, items, field) in metrics {
    let sums = daily(items, field, &window.dates, &window.tz);
    ins.total(metric, Some(sums.iter().sum::<i64>().into()));
    let points = window
      .dates
      .iter()
      .zip(sums)
      .map(|(d, v)| Point {
        date: d.to_string(),
        value: v.into(),
      })
      .collect();
    ins.series.push(Series {
      metric: metric.into(),
      points,
    });
  }
  let ratios: Vec<f64> = posts.iter().filter_map(|p| p.f64("upvote_ratio")).collect();
  if !ratios.is_empty() {
    let mean = ratios.iter().sum::<f64>() / ratios.len() as f64;
    ins.total("upvote_ratio", Some(mean.into()));
  }
  for (dimension, items) in [
    ("posts_by_subreddit", &posts),
    ("comments_by_subreddit", &comments),
  ] {
    let mut counts = BTreeMap::new();
    for d in items {
      *counts
        .entry(d.str("subreddit").unwrap_or_default())
        .or_default() += 1;
    }
    ins.breakdowns.extend(breakdown(dimension, counts));
  }
  Ok(())
}

/// Which totals are native and how the computed ones are made.
fn describe(ins: &mut Insights) {
  let x = &mut ins.extra;
  x.insert(
    "native".into(),
    json!([
      "karma",
      "post_karma",
      "comment_karma",
      "awarder_karma",
      "awardee_karma",
      "followers",
      "account_age_days",
      "trophies",
      "*karma_by_subreddit"
    ]),
  );
  x.insert(
    "computed".into(),
    "posts, comments and upvote_ratio over the window (and their series and \
     *_by_subreddit breakdowns) are computed from the account's own posts and comments \
     created in it: likes is the score, comments the comments received, both as they are \
     now, counted on the day the post or comment was created"
      .into(),
  );
}

/// Posts (`submitted`) or comments of `name` created since `since`, newest first.
async fn own(api: &Api, name: &str, which: &str, since: i64) -> Result<Vec<Value>> {
  let mut out = Vec::new();
  let mut req = PageReq {
    cursor: None,
    size: 100,
  };
  for _ in 0..MAX_PAGES {
    let query = listings::paged(vec![("sort", "new".into())], &req);
    let v = api.get(&format!("/user/{name}/{which}"), &query).await?;
    let items = v.list("data.children");
    out.extend(
      items
        .iter()
        .map(|t| t.at("data"))
        .filter(|d| d.i64("created_utc").unwrap_or(0) >= since)
        .cloned(),
    );
    let past = items
      .last()
      .is_some_and(|t| t.i64("data.created_utc").unwrap_or(0) < since);
    req.cursor = v.str("data.after");
    if past || req.cursor.is_none() {
      break;
    }
  }
  Ok(out)
}

/// A part the insights can do without: `null` and a note when it fails for
/// another reason than the session or the rate limit.
fn optional(r: Result<Value>, what: &str, missing: &mut Vec<String>) -> Result<Value> {
  match r {
    Err(e) if matches!(e.code, ErrorCode::NotAuthenticated | ErrorCode::RateLimited) => Err(e),
    Err(e) => {
      missing.push(format!("{what}: {}", e.message));
      Ok(Value::Null)
    }
    ok => ok,
  }
}

/// Daily sums of `field` (`None`: count items) per day of `dates`.
fn daily(items: &[Value], field: Option<&str>, dates: &[Date], tz: &TimeZone) -> Vec<i64> {
  let mut sums = vec![0; dates.len()];
  for d in items {
    let Some(date) = d.i64("created_utc").and_then(|s| day(s, tz)) else {
      continue;
    };
    if let Ok(i) = dates.binary_search(&date) {
      sums[i] += field.map_or(1, |f| d.i64(f).unwrap_or(0));
    }
  }
  sums
}

//! Reports of the official YouTube Analytics API (OAuth): totals, daily
//! series and distributions of the channel or one of its videos.

use social_core::{Breakdown, Insights, Result, Value, ValueExt, json};

use super::report::{Analytics, BREAKDOWNS, DAILY, METRICS, breakdown, label};
use super::{NO_REACH, public, soft, window};
use crate::refs;

/// Which reports [`fill`] asks for: the range of the totals and
/// distributions, the range of the daily series, and an optional video.
struct Scope<'a> {
  totals: (&'a str, &'a str),
  series: (&'a str, &'a str),
  video: Option<&'a str>,
}

/// Totals, daily series and distributions of the channel or one video.
async fn fill(analytics: &Analytics<'_>, ins: &mut Insights, scope: &Scope<'_>) -> Result<()> {
  let mut raw = serde_json::Map::new();
  fill_totals(analytics, ins, scope, &mut raw).await?;
  let daily = analytics
    .report(
      DAILY,
      Some("day"),
      scope.series,
      scope.video,
      Some("day"),
      None,
    )
    .await;
  if let Some(table) = soft(ins, "daily series", daily)? {
    raw.insert("daily".into(), table.raw.clone());
    ins.series = table.series();
  }
  fill_breakdowns(analytics, ins, scope, &mut raw).await?;
  ins.warnings.push(NO_REACH.into());
  ins.raw = Some(Value::Object(raw));
  Ok(())
}

/// Every metric over the whole range, and the net follower change.
async fn fill_totals(
  analytics: &Analytics<'_>,
  ins: &mut Insights,
  scope: &Scope<'_>,
  raw: &mut serde_json::Map<String, Value>,
) -> Result<()> {
  let all: Vec<&str> = METRICS.iter().map(|(m, _)| *m).collect();
  let totals = analytics
    .report(&all.join(","), None, scope.totals, scope.video, None, None)
    .await;
  let Some(table) = soft(ins, "totals", totals)? else {
    return Ok(());
  };
  raw.insert("totals".into(), table.raw.clone());
  for (k, v) in table.totals() {
    ins.total(&k, Some(v));
  }
  let gained = ins.totals.get("new_followers").and_then(Value::as_i64);
  let lost = ins.totals.get("lost_followers").and_then(Value::as_i64);
  if let (Some(g), Some(l)) = (gained, lost) {
    ins.total("net_followers", Some(Value::from(g - l)));
  }
  Ok(())
}

/// The distributions of [`BREAKDOWNS`] and the viewers' age and gender.
async fn fill_breakdowns(
  analytics: &Analytics<'_>,
  ins: &mut Insights,
  scope: &Scope<'_>,
  raw: &mut serde_json::Map<String, Value>,
) -> Result<()> {
  for (name, dim, metric) in BREAKDOWNS {
    let sort = format!("-{metric}");
    let report = analytics
      .report(
        metric,
        Some(dim),
        scope.totals,
        scope.video,
        Some(&sort),
        Some(25),
      )
      .await;
    if let Some(table) = soft(ins, name, report)? {
      raw.insert((*name).into(), table.raw.clone());
      let pairs = table
        .pairs()
        .into_iter()
        .map(|(l, v)| (label(dim, &l), v, (*dim == "country").then_some(l)))
        .collect();
      ins.breakdowns.push(breakdown(name, pairs));
    }
  }
  let demo = analytics
    .report(
      "viewerPercentage",
      Some("ageGroup,gender"),
      scope.totals,
      scope.video,
      None,
      None,
    )
    .await;
  if let Some(table) = soft(ins, "age and gender", demo)? {
    raw.insert("age_gender".into(), table.raw.clone());
    ins.breakdowns.extend(demographics(&table.rows));
  }
  Ok(())
}

/// `ageGroup, gender, viewerPercentage` rows → an age and a gender breakdown.
fn demographics(rows: &[Vec<Value>]) -> Vec<Breakdown> {
  let mut age: Vec<(String, f64)> = Vec::new();
  let mut gender: Vec<(String, f64)> = Vec::new();
  for r in rows {
    let (Some(a), Some(g), Some(p)) = (r[0].as_str(), r[1].as_str(), r[2].as_f64()) else {
      continue;
    };
    for (list, key) in [
      (&mut age, label("ageGroup", a)),
      (&mut gender, label("gender", g)),
    ] {
      match list.iter_mut().find(|(k, _)| *k == key) {
        Some(e) => e.1 += p / 100.0,
        None => list.push((key, p / 100.0)),
      }
    }
  }
  [("age", age), ("gender", gender)]
    .into_iter()
    .filter(|(_, l)| !l.is_empty())
    .map(|(dim, l)| {
      breakdown(
        dim,
        l.into_iter()
          .map(|(k, v)| (k, Value::from(v), None))
          .collect(),
      )
    })
    .collect()
}

pub(super) async fn account_report(analytics: &Analytics<'_>, days: u32) -> Result<Insights> {
  let channel = analytics.channel().await?;
  let id = channel.str("id").unwrap_or_default();
  let (from, to) = window(days);
  let range = (from.to_string(), to.to_string());
  let range = (range.0.as_str(), range.1.as_str());
  let mut ins = Insights {
    kind: "account".into(),
    title: channel.str("snippet.title"),
    url: Some(refs::channel_url(&id)),
    from: Some(range.0.to_owned()),
    to: Some(range.1.to_owned()),
    subject: id,
    ..Insights::default()
  };
  let scope = Scope {
    totals: range,
    series: range,
    video: None,
  };
  fill(analytics, &mut ins, &scope).await?;
  ins.total(
    "followers",
    channel.u64("statistics.subscriberCount").map(Value::from),
  );
  top_videos(analytics, &mut ins, range).await?;
  ins.extra.insert(
    "channel".into(),
    json!({
      "subscribers": channel.u64("statistics.subscriberCount"),
      "views": channel.u64("statistics.viewCount"),
      "videos": channel.u64("statistics.videoCount"),
    }),
  );
  if let Some(Value::Object(m)) = ins.raw.as_mut() {
    m.insert("channel".into(), channel);
  }
  Ok(ins)
}

/// The ten most viewed videos of the range, with their titles.
async fn top_videos(
  analytics: &Analytics<'_>,
  ins: &mut Insights,
  range: (&str, &str),
) -> Result<()> {
  let top = analytics
    .report(
      "views,estimatedMinutesWatched",
      Some("video"),
      range,
      None,
      Some("-views"),
      Some(10),
    )
    .await;
  let Some(table) = soft(ins, "top videos", top)? else {
    return Ok(());
  };
  let pairs = table.pairs();
  let ids: Vec<String> = pairs.iter().map(|(id, _)| id.clone()).collect();
  let titles = if ids.is_empty() {
    Vec::new()
  } else {
    analytics.videos(&ids).await.unwrap_or_default()
  };
  let title = |id: &str| {
    titles
      .iter()
      .find(|v| v.str("id").as_deref() == Some(id))
      .and_then(|v| v.str("snippet.title"))
  };
  let items = pairs
    .into_iter()
    .map(|(id, v)| (title(&id).unwrap_or_else(|| id.clone()), v, Some(id)))
    .collect();
  ins.breakdowns.push(breakdown("top_videos", items));
  Ok(())
}

pub(super) async fn video_report(
  analytics: &Analytics<'_>,
  arg: &str,
  days: u32,
) -> Result<Insights> {
  let id = refs::video(arg)?;
  let meta = analytics.videos(std::slice::from_ref(&id)).await?;
  let Some(meta) = meta.into_iter().next() else {
    return public(
      analytics.api,
      arg,
      vec!["the Data API does not list this video".into()],
    )
    .await;
  };
  let (from, to) = window(days);
  let published = meta
    .str("snippet.publishedAt")
    .and_then(|s| s.get(..10).map(str::to_owned))
    .unwrap_or_else(|| "2005-04-23".into());
  let start = from.to_string().max(published.clone());
  let (to, start) = (to.to_string(), start);
  let mut ins = Insights {
    kind: "post".into(),
    subject: id.clone(),
    title: meta.str("snippet.title"),
    url: Some(refs::video_url(&id)),
    from: Some(start.clone()),
    to: Some(to.clone()),
    totals_period: Some("lifetime".into()),
    ..Insights::default()
  };
  // Reports of other channels' videos come back empty or refused.
  let mine = analytics.channel().await?.str("id");
  if mine.is_none() || meta.str("snippet.channelId") != mine {
    let note = "not a video of the authorized channel: public counters only".to_owned();
    return public(analytics.api, arg, vec![note]).await;
  }
  let scope = Scope {
    totals: (&published, &to),
    series: (&start, &to),
    video: Some(&id),
  };
  fill(analytics, &mut ins, &scope).await?;
  for b in &mut ins.breakdowns {
    b.period = Some("lifetime".into());
  }
  if let Some(Value::Object(m)) = ins.raw.as_mut() {
    m.insert("video".into(), meta);
  }
  Ok(ins)
}

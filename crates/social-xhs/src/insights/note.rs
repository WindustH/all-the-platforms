//! The numbers of one of your notes, or the public counters of anyone else's.

use social_core::{Insights, Result, Value, ValueExt, json};

use super::{breakdown, days_ago, first_last, get, labelled, metrics, panel, rows, soft};
use crate::api::Api;
use crate::{notes, refs};

const NOTE_BASE: &str = "/api/galaxy/creator/datacenter/note/base";

const NOTE_RETENTION: &str = "/api/galaxy/creator/datacenter/note/analyze/audience/trend";

const NOTE_SOURCES: &str = "/api/galaxy/creator/datacenter/note/audience/source";

const NOTE_AUDIENCE: &str = "/api/galaxy/creator/datacenter/note/audience/source/detail";

pub async fn note(api: &Api, arg: &str, days: u32) -> Result<Insights> {
  api.require_login()?;
  let note = refs::note_ref(api, arg).await?;
  let base = match get(api, NOTE_BASE, &[("note_id", &note.id)]).await {
    Ok(v) if v.at("note_info").is_object() => v,
    Ok(_) => return public(api, arg).await,
    Err(e) if soft(&e) => return public(api, arg).await,
    Err(e) => return Err(e),
  };
  let info = base.at("note_info");
  let mut ins = Insights {
    kind: "post".into(),
    subject: note.id.clone(),
    title: info.first_str(&["title", "desc"]),
    url: Some(note.url()),
    ..Insights::default()
  };
  ins.extra.insert("scope".into(), json!("own"));
  // Totals cover the note's whole life; series only the last `days` days.
  ins.totals_period = Some("lifetime".into());
  let kind = info.str("type").map(|t| t.to_lowercase());
  if let Some(t) = &kind {
    ins.extra.insert("type".into(), json!(t));
  }
  metrics::totals(&mut ins, &base, metrics::NOTE);
  let since = days_ago(days);
  metrics::series(&mut ins, base.at("day"), metrics::NOTE, since.as_deref());
  ins.from = info.i64("post_time").and_then(metrics::day).max(since);
  ins.to = first_last(&ins).map(|(_, b)| b);
  metrics::diagnosis(&mut ins, "vs_similar_notes", base.list("analyse_infos"));

  let id = [("note_id", note.id.as_str())];
  let sources = panel(api, &mut ins, NOTE_SOURCES, &id).await?;
  breakdown(&mut ins, "traffic_source", &sources, "source", true);
  let audience = panel(api, &mut ins, NOTE_AUDIENCE, &id).await?;
  for dimension in ["gender", "age", "city", "interest"] {
    breakdown(&mut ins, dimension, &audience, dimension, true);
  }
  let mut raw = serde_json::Map::new();
  if kind.as_deref() == Some("video") {
    // 观看趋势: the share of viewers still watching at each second.
    let trend = panel(api, &mut ins, NOTE_RETENTION, &id).await?;
    let items = labelled(
      rows(&mut ins, "retention", &trend, "trend_list"),
      "date",
      |s| format!("{s}s"),
    );
    metrics::breakdown(&mut ins, "retention", &items, true);
    raw.insert("retention".into(), trend);
  }
  raw.insert("base".into(), base);
  raw.insert("sources".into(), sources);
  raw.insert("audience".into(), audience);
  ins.raw = Some(Value::Object(raw));
  Ok(ins)
}

/// Someone else's note: only its public counters.
async fn public(api: &Api, arg: &str) -> Result<Insights> {
  let post = notes::read(api, arg).await?;
  let mut ins = Insights {
    kind: "post".into(),
    subject: post.id.clone(),
    title: post.title.clone(),
    url: post.url.clone(),
    ..Insights::default()
  };
  let metrics = &post.metrics;
  for (name, value) in [
    ("likes", metrics.likes),
    ("comments", metrics.comments),
    ("shares", metrics.shares),
    ("favorites", metrics.favorites),
  ] {
    ins.total(name, value.map(Value::from));
  }
  for (name, value) in &metrics.other {
    ins.total(name, Some(Value::from(*value)));
  }
  ins.extra.insert("scope".into(), json!("public"));
  ins.warnings.push(
    "not one of your notes: the creator center has data only for your own, these are its public counters".into(),
  );
  ins.raw = post.raw;
  Ok(ins)
}

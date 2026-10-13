//! `insights`: the statistics Telegram keeps for channels and supergroups
//! the account administers (large enough ones: Telegram enables them from
//! a few hundred subscribers), and for single channel posts.
//!
//! Headline numbers are current values with the previous period next to
//! them; graphs over dates become series, graphs by source, language or
//! reaction become breakdowns summed over the period.

use grammers_client::{InvocationError, tl};
use social_core::text::{from_millis, from_secs};
use social_core::{
  Breakdown, Error, Insights, Point, Result, Series, Share, Value, ValueExt, json,
};

use crate::mt::{self, Conn, error};
use crate::refs::{self, Chat};

pub async fn channel(conn: &Conn, arg: &str, days: u32) -> Result<Insights> {
  if let Ok(m) = refs::message(arg) {
    return message(conn, &m.chat, m.id).await;
  }
  let chat = refs::chat(arg)?;
  let peer = mt::peer(conn, &chat).await?;
  let mut out = Insights {
    kind: "account".into(),
    subject: chat.to_string(),
    url: chat.username().map(|u| format!("https://t.me/{u}")),
    ..Insights::default()
  };
  let broadcast = tl::functions::stats::GetBroadcastStats {
    dark: false,
    channel: peer.into(),
  };
  match invoke(conn, &broadcast).await {
    Ok(tl::enums::stats::BroadcastStats::Stats(s)) => broadcast_stats(conn, &mut out, s).await,
    Err(e) if e.message.contains("MEGAGROUP") || e.message.contains("BROADCAST_REQUIRED") => {
      let request = tl::functions::stats::GetMegagroupStats {
        dark: false,
        channel: peer.into(),
      };
      let tl::enums::stats::MegagroupStats::Stats(s) = invoke(conn, &request).await?;
      group_stats(conn, &mut out, s).await;
    }
    Err(e) => return Err(stats_error(e)),
  }
  trim(&mut out, days);
  Ok(out)
}

async fn message(conn: &Conn, chat: &Chat, id: i32) -> Result<Insights> {
  let peer = mt::peer(conn, chat).await?;
  let request = tl::functions::stats::GetMessageStats {
    dark: false,
    channel: peer.into(),
    msg_id: id,
  };
  let tl::enums::stats::MessageStats::Stats(s) =
    invoke(conn, &request).await.map_err(stats_error)?;
  let mut out = Insights {
    kind: "post".into(),
    subject: format!("{chat}/{id}"),
    url: chat.username().map(|u| format!("https://t.me/{u}/{id}")),
    ..Insights::default()
  };
  graph(conn, &mut out, "views", s.views_graph).await;
  graph(
    conn,
    &mut out,
    "reactions_by_emotion",
    s.reactions_by_emotion_graph,
  )
  .await;
  Ok(out)
}

async fn broadcast_stats(conn: &Conn, out: &mut Insights, s: tl::types::stats::BroadcastStats) {
  period(out, &s.period);
  abs(out, "followers", &s.followers);
  abs(out, "views_per_post", &s.views_per_post);
  abs(out, "shares_per_post", &s.shares_per_post);
  abs(out, "reactions_per_post", &s.reactions_per_post);
  abs(out, "views_per_story", &s.views_per_story);
  abs(out, "shares_per_story", &s.shares_per_story);
  abs(out, "reactions_per_story", &s.reactions_per_story);
  let tl::enums::StatsPercentValue::Value(n) = &s.enabled_notifications;
  if n.total > 0.0 {
    out.total("notifications_enabled_rate", json!(n.part / n.total));
  }
  let recent: Vec<Value> = s
    .recent_posts_interactions
    .iter()
    .filter_map(|p| match p {
      tl::enums::PostInteractionCounters::Message(m) => Some(json!({
        "message_id": m.msg_id, "views": m.views, "shares": m.forwards, "reactions": m.reactions,
      })),
      _ => None,
    })
    .collect();
  if !recent.is_empty() {
    out
      .extra
      .insert("recent_posts".into(), Value::Array(recent));
  }
  for (name, g) in [
    ("growth", s.growth_graph),
    ("followers", s.followers_graph),
    ("muted", s.mute_graph),
    ("hour_of_day", s.top_hours_graph),
    ("interactions", s.interactions_graph),
    ("instant_view", s.iv_interactions_graph),
    ("views_by_source", s.views_by_source_graph),
    ("new_followers_by_source", s.new_followers_by_source_graph),
    ("language", s.languages_graph),
    ("reactions_by_emotion", s.reactions_by_emotion_graph),
    ("story_interactions", s.story_interactions_graph),
    (
      "story_reactions_by_emotion",
      s.story_reactions_by_emotion_graph,
    ),
  ] {
    graph(conn, out, name, g).await;
  }
}

async fn group_stats(conn: &Conn, out: &mut Insights, s: tl::types::stats::MegagroupStats) {
  period(out, &s.period);
  abs(out, "members", &s.members);
  abs(out, "messages", &s.messages);
  abs(out, "viewers", &s.viewers);
  abs(out, "posters", &s.posters);
  let posters: Vec<Value> = s
    .top_posters
    .iter()
    .map(|tl::enums::StatsGroupTopPoster::Poster(p)| {
      json!({"user_id": p.user_id, "messages": p.messages, "avg_chars": p.avg_chars})
    })
    .collect();
  if !posters.is_empty() {
    out
      .extra
      .insert("top_posters".into(), Value::Array(posters));
  }
  for (name, g) in [
    ("growth", s.growth_graph),
    ("members", s.members_graph),
    ("new_members_by_source", s.new_members_by_source_graph),
    ("language", s.languages_graph),
    ("messages", s.messages_graph),
    ("actions", s.actions_graph),
    ("hour_of_day", s.top_hours_graph),
    ("weekday", s.weekdays_graph),
  ] {
    graph(conn, out, name, g).await;
  }
}

/// Stats live in a datacenter of their own (`STATS_MIGRATE_<dc>`).
async fn invoke<R: tl::RemoteCall>(conn: &Conn, request: &R) -> Result<R::Return> {
  match conn.client.invoke(request).await {
    Err(InvocationError::Rpc(e)) if e.name == "STATS_MIGRATE" => {
      let dc = e.value.unwrap_or(0) as i32;
      conn.client.invoke_in_dc(dc, request).await.map_err(error)
    }
    other => other.map_err(error),
  }
}

fn stats_error(e: Error) -> Error {
  if e.message.contains("CHAT_ADMIN_REQUIRED") || e.message.contains("STATS") {
    e.with_hint("statistics are for admins of channels and groups Telegram enabled them for")
  } else {
    e
  }
}

fn period(out: &mut Insights, p: &tl::enums::StatsDateRangeDays) {
  let tl::enums::StatsDateRangeDays::Days(d) = p;
  let day = |s: i32| from_secs(s.into()).map(|t| t.strftime("%Y-%m-%d").to_string());
  out.from = day(d.min_date);
  out.to = day(d.max_date);
}

fn abs(out: &mut Insights, name: &str, v: &tl::enums::StatsAbsValueAndPrev) {
  let tl::enums::StatsAbsValueAndPrev::Prev(v) = v;
  out.total(name, json!(v.current));
  out.total(&format!("{name}_previous"), json!(v.previous));
}

/// One graph: async ones are loaded first; errors become warnings.
async fn graph(conn: &Conn, out: &mut Insights, name: &str, g: tl::enums::StatsGraph) {
  let g = match g {
    tl::enums::StatsGraph::Async(a) => {
      let request = tl::functions::stats::LoadAsyncGraph {
        token: a.token,
        x: None,
      };
      match invoke(conn, &request).await {
        Ok(g) => g,
        Err(e) => {
          out.warnings.push(format!("{name}: {}", e.message));
          return;
        }
      }
    }
    other => other,
  };
  match g {
    tl::enums::StatsGraph::Graph(g) => {
      let tl::enums::DataJson::Json(data) = g.json;
      match serde_json::from_str::<Value>(&data.data) {
        Ok(v) => add_graph(out, name, &v),
        Err(e) => out.warnings.push(format!("{name}: unreadable graph ({e})")),
      }
    }
    tl::enums::StatsGraph::Error(e) => out.warnings.push(format!("{name}: {}", e.error)),
    tl::enums::StatsGraph::Async(_) => {}
  }
}

/// `{"columns": [["x", ms…], ["y0", n…]…], "names": {"y0": "…"}}`.
fn add_graph(out: &mut Insights, name: &str, v: &Value) {
  let columns = v.list("columns");
  let Some(x) = columns.iter().find(|c| c.str("0").as_deref() == Some("x")) else {
    return;
  };
  let xs: Vec<i64> = x
    .list("")
    .iter()
    .skip(1)
    .filter_map(|n| n.as_i64())
    .collect();
  let by_category = name.contains("by_") || name == "language";
  let hourly = name == "hour_of_day";
  let mut items = Vec::new();
  for col in columns
    .iter()
    .filter(|c| c.str("0").as_deref() != Some("x"))
  {
    let key = col.str("0").unwrap_or_default();
    let label = v
      .str(&format!("names.{key}"))
      .unwrap_or_else(|| key.clone());
    let values: Vec<f64> = col
      .list("")
      .iter()
      .skip(1)
      .filter_map(|n| n.as_f64())
      .collect();
    if by_category {
      items.push(Share {
        label,
        value: json!(values.iter().sum::<f64>()),
        ..Share::default()
      });
      continue;
    }
    let points = xs
      .iter()
      .zip(&values)
      .map(|(x, y)| Point {
        date: if hourly {
          format!("{x:02}:00")
        } else {
          from_millis(*x)
            .map(|t| t.strftime("%Y-%m-%d").to_string())
            .unwrap_or_else(|| x.to_string())
        },
        value: json!(y),
      })
      .collect();
    out.series.push(Series {
      metric: format!("{name}.{}", snake(&label)),
      points,
    });
  }
  if !items.is_empty() {
    let total: f64 = items.iter().filter_map(|i| i.value.as_f64()).sum();
    for i in &mut items {
      i.ratio = i.value.as_f64().filter(|_| total > 0.0).map(|n| n / total);
    }
    out.breakdowns.push(Breakdown {
      dimension: name.into(),
      items,
      ..Breakdown::default()
    });
  }
}

fn snake(label: &str) -> String {
  let s: String = label
    .chars()
    .map(|c| {
      if c.is_alphanumeric() {
        c.to_ascii_lowercase()
      } else {
        '_'
      }
    })
    .collect();
  s.split('_')
    .filter(|p| !p.is_empty())
    .collect::<Vec<_>>()
    .join("_")
}

/// Keep the last `days` days of daily series.
fn trim(out: &mut Insights, days: u32) {
  let Some(cut) = jiff::Timestamp::now()
    .checked_sub(jiff::SignedDuration::from_hours(24 * i64::from(days)))
    .ok()
    .map(|t| t.strftime("%Y-%m-%d").to_string())
  else {
    return;
  };
  for s in &mut out.series {
    if s.points.first().is_some_and(|p| p.date.len() == 10) {
      s.points.retain(|p| p.date >= cut);
    }
  }
}

//! Your listening: top tracks and artists over one of Spotify's three
//! windows, which `--days` is rounded to.

use atp_core::{Breakdown, Error, Insights, Result, Share, Value, ValueExt, json};

use crate::api::Api;
use crate::parse;

/// Spotify ranks at most 50 items a call.
const TOP: &str = "50";

/// The window nearest `days`: Spotify's `time_range` and what it covers.
fn range(days: u32) -> (&'static str, &'static str) {
  match days {
    0..90 => ("short_term", "about the last 4 weeks"),
    90..270 => ("medium_term", "about the last 6 months"),
    _ => ("long_term", "about the last year"),
  }
}

/// Top tracks and top artists of the logged-in account.
pub async fn top(api: &Api, post: Option<&str>, days: u32) -> Result<Insights> {
  if post.is_some() {
    return Err(Error::input(
      "Spotify has no numbers for single items; run insights without one",
    ));
  }
  let (range, period) = range(days);
  let query = [("time_range", range.to_owned()), ("limit", TOP.to_owned())];
  let tracks = api.get("/me/top/tracks", &query).await?;
  let artists = api.get("/me/top/artists", &query).await?;
  let me = api.me().await?;
  let mut insights = Insights {
    kind: "account".into(),
    subject: me.first_str(&["display_name", "id"]).unwrap_or_default(),
    title: Some("Your top tracks and artists".into()),
    url: me.str("external_urls.spotify"),
    breakdowns: vec![
      ranking("top_tracks", period, tracks.list("items"), track_label),
      ranking("top_artists", period, artists.list("items"), artist_label),
    ],
    ..Insights::default()
  };
  insights.extra.insert("time_range".into(), json!(range));
  if tracks.list("items").is_empty() && artists.list("items").is_empty() {
    insights
      .warnings
      .push("Spotify has too little listening in this window to rank anything".into());
  }
  Ok(insights)
}

/// A ranked list: the label of each item and its place (1 is the top).
fn ranking(
  dimension: &str,
  period: &str,
  items: &[Value],
  label: fn(&Value) -> Option<String>,
) -> Breakdown {
  let items = items
    .iter()
    .filter_map(|v| Some((v.str("id")?, label(v)?)))
    .enumerate()
    .map(|(i, (id, label))| Share {
      label,
      id: Some(id),
      value: json!(i + 1),
      ratio: None,
    })
    .collect();
  Breakdown {
    dimension: dimension.into(),
    period: Some(period.into()),
    items,
  }
}

/// `Title — Artist / Artist`.
fn track_label(v: &Value) -> Option<String> {
  let track = parse::track(v)?;
  let title = track.title.unwrap_or_default();
  Some(match track.author {
    Some(author) => format!("{title} — {}", author.name),
    None => title,
  })
}

fn artist_label(v: &Value) -> Option<String> {
  v.str("name")
}

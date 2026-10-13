//! Insights of a business or creator account through the Graph API: an
//! Instagram-login token (`IG…`, graph.instagram.com) or a Facebook token of
//! the Page the account is linked to (graph.facebook.com). Asked one metric
//! at a time, since Meta retires names and one unknown name fails a request.

use atp_core::{Breakdown, Error, Insights, Point, Result, Series, Share, Value, ValueExt, json};
use atp_meta::Graph;
use atp_meta::graph::{FACEBOOK, INSTAGRAM, Params};
use jiff::{Timestamp, ToSpan};

use crate::client::Client;
use crate::refs;

const HINT: &str = "set INSTAGRAM_ACCESS_TOKEN (an Instagram-login token, or a Facebook token of the \
   linked Page with instagram_basic and instagram_manage_insights) or run \
   `atp instagram login --token <token>`; INSTAGRAM_USER_ID picks the account";
/// Account metrics summed over the window.
const TOTALS: &[&str] = &[
  "views",
  "reach",
  "accounts_engaged",
  "total_interactions",
  "likes",
  "comments",
  "shares",
  "saves",
  "replies",
  "follows_and_unfollows",
  "profile_links_taps",
];
const MEDIA: &[&str] = &[
  "views",
  "reach",
  "likes",
  "comments",
  "shares",
  "saved",
  "total_interactions",
  "profile_visits",
  "follows",
  "ig_reels_avg_watch_time",
  "ig_reels_video_view_total_time",
];

fn s(v: &str) -> String {
  v.to_owned()
}

/// The Graph client and the Instagram account id to ask about.
async fn account(c: &Client) -> Result<(Graph<'_>, String)> {
  let token = c
    .token()
    .ok_or_else(|| Error::auth("no Graph API token for Instagram insights").with_hint(HINT))?;
  let ig_login = token.starts_with("IG");
  let g = Graph::new(&c.ctx, if ig_login { INSTAGRAM } else { FACEBOOK }, token);
  if let Ok(id) = std::env::var("INSTAGRAM_USER_ID")
    && !id.trim().is_empty()
  {
    return Ok((g, id.trim().to_owned()));
  }
  if ig_login {
    let v = g.get("me", &[(s("fields"), s("user_id,username"))]).await?;
    let id = v
      .str("user_id")
      .or_else(|| v.str("id"))
      .ok_or_else(|| Error::upstream("the token names no Instagram account"))?;
    return Ok((g, id));
  }
  let pages = g
    .all(
      "me/accounts",
      &[(s("fields"), s("instagram_business_account{id,username}"))],
      0,
    )
    .await?;
  let id = pages
    .iter()
    .find_map(|p| p.str("instagram_business_account.id"))
    .ok_or_else(|| {
      Error::not_found("no Instagram business account linked to the token's Pages").with_hint(HINT)
    })?;
  Ok((g, id))
}

pub async fn insights(c: &Client, post: Option<&str>, days: u32) -> Result<Insights> {
  let (g, ig) = account(c).await?;
  match post {
    Some(p) => media_insights(&g, &ig, p).await,
    None => account_insights(&g, &ig, days).await,
  }
}

async fn account_insights(g: &Graph<'_>, ig: &str, days: u32) -> Result<Insights> {
  let info = g
    .get(
      ig,
      &[(
        s("fields"),
        s("username,name,followers_count,follows_count,media_count"),
      )],
    )
    .await?;
  let days = days.clamp(1, 30);
  let until = Timestamp::now();
  let since = until - (i64::from(days) * 24).hours();
  let mut ins = Insights {
    kind: "account".into(),
    subject: info.str("username").unwrap_or_else(|| ig.to_owned()),
    title: info.str("name"),
    url: info
      .str("username")
      .map(|u| format!("https://www.instagram.com/{u}/")),
    from: Some(day(since)),
    to: Some(day(until)),
    ..Insights::default()
  };
  ins.total("followers", info.at("followers_count").clone());
  ins.total("following", info.at("follows_count").clone());
  ins.total("posts", info.at("media_count").clone());
  let window = |metric: &str, extra: &[(&str, &str)]| {
    let mut q: Params = vec![
      (s("metric"), s(metric)),
      (s("period"), s("day")),
      (s("since"), since.as_second().to_string()),
      (s("until"), until.as_second().to_string()),
    ];
    q.extend(extra.iter().map(|(k, v)| (s(k), s(v))));
    q
  };
  for metric in TOTALS {
    let q = window(metric, &[("metric_type", "total_value")]);
    match g.get(&format!("{ig}/insights"), &q).await {
      Ok(v) => {
        let value = v.at("data.0.total_value.value").clone();
        ins.total(metric, value);
        for b in v.list("data.0.total_value.breakdowns") {
          ins.breakdowns.push(breakdown(metric, b, None));
        }
      }
      Err(e) => ins.warnings.push(format!("{metric}: {}", e.message)),
    }
  }
  for metric in ["reach", "follower_count"] {
    match g.get(&format!("{ig}/insights"), &window(metric, &[])).await {
      Ok(v) => {
        let points: Vec<Point> = v
          .list("data.0.values")
          .iter()
          .filter_map(|p| {
            Some(Point {
              date: p.str("end_time")?.chars().take(10).collect(),
              value: p.at("value").clone(),
            })
          })
          .collect();
        if !points.is_empty() {
          let name = if metric == "follower_count" {
            "new_followers"
          } else {
            metric
          };
          ins.series.push(Series {
            metric: name.into(),
            points,
          });
        }
      }
      Err(e) => ins.warnings.push(format!("{metric} by day: {}", e.message)),
    }
  }
  for dim in ["age", "gender", "country", "city"] {
    let q: Params = vec![
      (s("metric"), s("follower_demographics")),
      (s("period"), s("lifetime")),
      (s("metric_type"), s("total_value")),
      (s("timeframe"), s("this_month")),
      (s("breakdown"), s(dim)),
    ];
    match g.get(&format!("{ig}/insights"), &q).await {
      Ok(v) => {
        for b in v.list("data.0.total_value.breakdowns") {
          ins
            .breakdowns
            .push(breakdown(&format!("followers_{dim}"), b, Some("lifetime")));
        }
      }
      Err(e) => ins.warnings.push(format!("follower {dim}: {}", e.message)),
    }
  }
  Ok(ins)
}

/// `{dimension_keys, results: [{dimension_values, value}]}` as a breakdown.
fn breakdown(metric: &str, b: &Value, period: Option<&str>) -> Breakdown {
  let keys: Vec<String> = b
    .list("dimension_keys")
    .iter()
    .filter_map(Value::as_str)
    .map(str::to_owned)
    .collect();
  Breakdown {
    dimension: if keys.is_empty() || metric.starts_with("followers_") {
      metric.to_owned()
    } else {
      format!("{metric}_by_{}", keys.join("_"))
    },
    period: period.map(str::to_owned),
    items: b
      .list("results")
      .iter()
      .map(|r| Share {
        label: r
          .list("dimension_values")
          .iter()
          .filter_map(|x| {
            x.as_str()
              .map(str::to_owned)
              .or_else(|| Some(x.to_string()))
          })
          .collect::<Vec<_>>()
          .join(" / "),
        value: r.at("value").clone(),
        ..Share::default()
      })
      .collect(),
  }
}

/// A post's insights. The Graph API has its own media ids: a link or
/// shortcode is looked up among the account's recent media.
async fn media_insights(g: &Graph<'_>, ig: &str, arg: &str) -> Result<Insights> {
  let id = graph_media_id(g, ig, arg).await?;
  let info = g
    .get(
      &id,
      &[(
        s("fields"),
        s("id,caption,permalink,media_type,media_product_type,timestamp,like_count,comments_count"),
      )],
    )
    .await?;
  let mut ins = Insights {
    kind: "post".into(),
    subject: id.clone(),
    title: info.str("caption").map(|c| c.chars().take(60).collect()),
    url: info.str("permalink"),
    totals_period: Some("lifetime".into()),
    ..Insights::default()
  };
  ins.total("likes", info.at("like_count").clone());
  ins.total("comments", info.at("comments_count").clone());
  for metric in MEDIA {
    let q: Params = vec![(s("metric"), s(metric))];
    match g.get(&format!("{id}/insights"), &q).await {
      Ok(v) => {
        let value = v.at("data.0.values.0.value").clone();
        let value = if value.is_null() {
          v.at("data.0.total_value.value").clone()
        } else {
          value
        };
        let name = match *metric {
          "ig_reels_avg_watch_time" => "avg_watch_seconds",
          "ig_reels_video_view_total_time" => "watch_seconds",
          "saved" => "favorites",
          m => m,
        };
        // Watch times come in milliseconds.
        let value = if metric.starts_with("ig_reels") {
          value.as_f64().map(|ms| json!(ms / 1000.0)).unwrap_or(value)
        } else {
          value
        };
        ins.total(name, value);
      }
      Err(e) => ins.warnings.push(format!("{metric}: {}", e.message)),
    }
  }
  Ok(ins)
}

async fn graph_media_id(g: &Graph<'_>, ig: &str, arg: &str) -> Result<String> {
  let a = arg.trim();
  if a.len() >= 15 && a.bytes().all(|b| b.is_ascii_digit()) && a.starts_with('1') {
    return Ok(a.to_owned());
  }
  let code = refs::shortcode(a)?;
  let items = g
    .all(
      &format!("{ig}/media"),
      &[(s("fields"), s("id,shortcode,permalink"))],
      500,
    )
    .await?;
  items
    .iter()
    .find(|m| {
      m.str("shortcode").as_deref() == Some(code.as_str())
        || m
          .str("permalink")
          .is_some_and(|p| p.contains(&format!("/{code}/")))
    })
    .and_then(|m| m.str("id"))
    .ok_or_else(|| {
      Error::not_found(format!(
        "post {code} is not among the account's latest 500 posts (insights cover your own posts only)"
      ))
    })
}

fn day(t: Timestamp) -> String {
  t.strftime("%Y-%m-%d").to_string()
}

//! Analytics.
//!
//! Creator analytics come from the official YouTube Analytics API with OAuth
//! credentials in the environment ([`oauth`]): totals, daily series and
//! distributions (traffic source, playback location, device, OS, subscribed
//! status, region, sharing service, age, gender, top videos) of the channel
//! or of one of its videos. YouTube Studio's own backend is not used: its
//! endpoints are only discoverable from bundles served to logged-in sessions.
//! Impressions and click-through rate exist only in Studio (and the
//! Reporting API's bulk "reach" reports), so they are reported as missing.
//!
//! Without OAuth, a logged-in account gets its channel's public numbers and
//! its recent uploads; other people's videos always get their public counters.

mod oauth;
mod official;
mod report;

use jiff::ToSpan;
use jiff::civil::Date;
use jiff::tz::TimeZone;
use social_core::{Breakdown, ErrorCode, Insights, PageReq, Result, Share, Value};

use self::oauth::{Credentials, SETUP_HINT};
use self::report::Analytics;
use crate::api::Api;
use crate::channels::Tab;
use crate::{account, channels, refs, videos};

pub use self::oauth::authorize;

const NO_REACH: &str = "impressions and click-through rate are shown in YouTube Studio only; the Analytics API does not report them";

/// Days as YouTube Analytics counts them (Pacific time): `days` up to today.
fn window(days: u32) -> (Date, Date) {
  let tz = TimeZone::get("America/Los_Angeles").unwrap_or(TimeZone::UTC);
  let today = jiff::Timestamp::now().to_zoned(tz).date();
  (today.saturating_sub((i64::from(days) - 1).days()), today)
}

/// A part the insights can do without: failures other than the session, the
/// rate limit or the network become warnings.
fn soft<T>(ins: &mut Insights, what: &str, result: Result<T>) -> Result<Option<T>> {
  match result {
    Ok(v) => Ok(Some(v)),
    Err(e)
      if matches!(
        e.code,
        ErrorCode::NotAuthenticated | ErrorCode::RateLimited | ErrorCode::NetworkError
      ) =>
    {
      Err(e)
    }
    Err(e) => {
      ins.warnings.push(format!("{what}: {}", e.message));
      Ok(None)
    }
  }
}

pub async fn insights(api: &Api, post: Option<&str>, days: u32) -> Result<Insights> {
  match (post, Credentials::from_env()) {
    (Some(p), Some(c)) => official::video_report(&Analytics { api, creds: &c }, p, days).await,
    (None, Some(c)) => official::account_report(&Analytics { api, creds: &c }, days).await,
    (Some(p), None) => {
      let note = format!("public counters only: {SETUP_HINT}");
      public(api, p, vec![note]).await
    }
    (None, None) => account_snapshot(api, days).await,
  }
}

/// Current public counters of any video or post.
async fn public(api: &Api, arg: &str, warnings: Vec<String>) -> Result<Insights> {
  let post = videos::read(api, arg).await?;
  let m = &post.metrics;
  let mut ins = Insights {
    kind: "post".into(),
    subject: post.id.clone(),
    title: post.title.clone().or_else(|| {
      post
        .text
        .as_deref()
        .map(|t| social_core::text::truncate(&social_core::text::one_line(t), 80))
    }),
    url: post.url.clone(),
    totals_period: Some("lifetime".into()),
    warnings,
    ..Insights::default()
  };
  for (key, n) in [
    ("views", m.views),
    ("likes", m.likes),
    ("comments", m.comments),
  ] {
    ins.total(key, n.map(Value::from));
  }
  if let Some(f) = post.author.as_ref().and_then(|a| a.stats.followers) {
    ins.extra.insert("channel_subscribers".into(), f.into());
  }
  ins.raw = post.raw;
  Ok(ins)
}

/// Without OAuth: the logged-in channel's public numbers and the uploads of
/// the window with their current views.
async fn account_snapshot(api: &Api, days: u32) -> Result<Insights> {
  if !api.logged_in() {
    return Err(
      social_core::Error::auth("insights need a logged-in account or OAuth credentials")
        .with_hint(SETUP_HINT),
    );
  }
  let me = account::whoami(api).await?;
  let (from, to) = window(days);
  let mut ins = Insights {
    kind: "account".into(),
    subject: me.id.clone(),
    title: Some(me.name.clone()),
    url: me.url.clone(),
    from: Some(from.to_string()),
    to: Some(to.to_string()),
    totals_period: Some("lifetime".into()),
    warnings: vec![format!(
      "channel totals and recent uploads only; watch time, audience and traffic sources: {SETUP_HINT}"
    )],
    ..Insights::default()
  };
  ins.total("followers", me.stats.followers.map(Value::from));
  ins.total(
    "views",
    me.stats.other.get("views").copied().map(Value::from),
  );
  ins.total("videos", me.stats.posts.map(Value::from));
  if refs::is_channel_id(&me.id) {
    recent_uploads(api, &mut ins, &me.id, days).await?;
  }
  ins.raw = me.raw;
  Ok(ins)
}

/// Current views of the channel's uploads of the last `days` days.
async fn recent_uploads(api: &Api, ins: &mut Insights, channel: &str, days: u32) -> Result<()> {
  let since = jiff::Timestamp::now()
    .checked_sub(jiff::SignedDuration::from_hours(24 * i64::from(days)))
    .ok();
  let page = PageReq {
    cursor: None,
    size: 30,
  };
  let uploads = channels::tab(api, channel, Tab::Videos, &page).await;
  let Some(listing) = soft(ins, "uploads", uploads)? else {
    return Ok(());
  };
  let recent: Vec<Share> = listing
    .posts
    .into_iter()
    .filter(|p| p.created_at.zip(since).is_some_and(|(at, s)| at >= s))
    .map(|p| Share {
      label: p.title.unwrap_or_else(|| p.id.clone()),
      value: p.metrics.views.into(),
      id: Some(p.id),
      ..Share::default()
    })
    .collect();
  ins.extra.insert(
    "uploads_views".into(),
    "current views of the uploads of the window (upload times as YouTube rounds them)".into(),
  );
  ins.breakdowns.push(Breakdown {
    dimension: "uploads_views".into(),
    items: recent,
    ..Breakdown::default()
  });
  Ok(())
}

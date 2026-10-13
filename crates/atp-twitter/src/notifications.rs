//! Notifications (`NotificationsTimeline`): aggregated events (likes,
//! reposts, follows ...) and the tweets that mention or answer you.

use atp_core::{Notification, Page, PageReq, Result, json};

use crate::api::Api;
use crate::graphql::NOTIFICATIONS;
use crate::parse;
use crate::timeline::{self, Timeline, vars, with};

const PATHS: &[&str] =
  &["data.viewer_v2.user_results.result.notification_timeline.timeline.instructions"];

/// `kind`: all (default), verified or mentions.
pub async fn list(api: &Api, kind: Option<&str>, req: &PageReq) -> Result<Page<Notification>> {
  api.require_login()?;
  let timeline_type = match kind {
    Some("verified") => "Verified",
    Some("mentions") => "Mentions",
    _ => "All",
  };
  let variables = with(vars(req), json!({ "timeline_type": timeline_type }));
  timeline::page(
    api,
    &NOTIFICATIONS,
    variables,
    PATHS,
    req,
    |tl: &Timeline| {
      tl.items()
        .filter_map(|(id, item)| parse::notification(id, item))
        .collect()
    },
  )
  .await
}

//! The activity inbox: likes, comments, follows, mentions.

use atp_core::{Notification, Page, Result, ValueExt};

use crate::api::Api;
use crate::parse;

pub async fn list(api: &Api) -> Result<Page<Notification>> {
  api.require_session("notifications")?;
  let v = api.get("news/inbox/", &[]).await?;
  let mut out = Vec::new();
  for (unread, key) in [(true, "new_stories"), (false, "old_stories")] {
    out.extend(
      v.list(key)
        .iter()
        .map(|story| parse::notification(story, unread)),
    );
  }
  Ok(Page::last(out))
}

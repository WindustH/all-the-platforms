//! Notifications by kind and private-message threads, from the app's
//! timelines; `all` stays on the web's recent list ([`people::notifications`]).

use social_core::{Ctx, Error, Notification, Page, PageReq, Result, ValueExt};

use crate::api::{self, MOBILE};
use crate::{parse, people};

/// Kinds for `notifications --type`.
pub const KINDS: &[&str] = &["all", "comment", "like", "favorite", "follow", "message"];

pub async fn notifications(
  ctx: &Ctx,
  kind: Option<&str>,
  page: &PageReq,
) -> Result<Page<Notification>> {
  let path = match kind.unwrap_or("all") {
    "all" => return people::notifications(ctx, page).await,
    "message" => "/notifications/v3/message/v3".to_owned(),
    "favorite" => "/notifications/v3/timeline/entry/favlist_me".to_owned(),
    k @ ("comment" | "like" | "follow") => format!("/notifications/v3/timeline/entry/{k}"),
    other => {
      return Err(Error::input(format!(
        "unknown notification type `{other}`; one of {}",
        KINDS.join(", ")
      )));
    }
  };
  let req = match &page.cursor {
    Some(query) => api::mobile(ctx, &format!("{MOBILE}{path}?{query}")),
    None => api::mobile(ctx, &path).query("limit", page.size_within(20).max(10)),
  };
  let v = api::call(ctx, req).await?;
  let items = v.list("data").iter().filter_map(parse::card).collect();
  Ok(Page::new(items, api::next_query(&v)))
}

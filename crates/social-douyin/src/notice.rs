//! The notice center (互动消息) of the logged-in account.

use social_core::{Ctx, Error, Notification, Page, PageReq, Result, ValueExt};

use crate::{api, parse};

const NOTICE: &str = "/aweme/v1/web/notice/";

pub const KINDS: &[&str] = &["all", "follower", "mention", "comment", "like", "danmaku"];

/// `notice_group` of each kind.
fn group(kind: Option<&str>) -> Result<&'static str> {
  Ok(match kind.unwrap_or("all") {
    "all" => "700",
    "follower" => "401",
    "mention" => "601",
    "comment" => "2",
    "like" => "3",
    "danmaku" => "520",
    other => return Err(Error::input(format!("unknown notification type: {other}"))),
  })
}

/// Pages continue from the `min_time` / `max_time` pair the last one returned.
pub async fn list(ctx: &Ctx, kind: Option<&str>, page: &PageReq) -> Result<Page<Notification>> {
  api::require_login(ctx)?;
  let (min, max) = page
    .cursor
    .as_deref()
    .and_then(|c| c.split_once('-'))
    .unwrap_or(("0", "0"));
  let v = api::get(ctx, NOTICE)
    .arg("is_new_notice", "1")
    .arg("is_mark_read", "1")
    .arg("notice_group", group(kind)?)
    .arg("count", page.size_within(20))
    .arg("min_time", min)
    .arg("max_time", max)
    .send()
    .await?;
  // `notice_list` stays empty; the messages are in `notice_list_v2`.
  let rows = match v.list("notice_list_v2") {
    [] => v.list("notice_list"),
    rows => rows,
  };
  let items: Vec<Notification> = rows.iter().map(parse::notice).collect();
  let next = (v.i64("has_more") == Some(1) && !items.is_empty())
    .then(|| Some(format!("{}-{}", v.str("min_time")?, v.str("max_time")?)))
    .flatten();
  Ok(Page::new(items, next))
}

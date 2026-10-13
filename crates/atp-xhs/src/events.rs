//! The creator center's activity center (活动中心): official campaigns that
//! creators join by publishing a note for them, and the activities they keep.
//!
//! Endpoints from the creator center bundle: `EVENTS_LIST` and
//! `COLLECT_MODIFY` in the index API list, called by chunk 6808. There is no
//! detail endpoint: the page shows the list entries. Joining is publishing a
//! note "associated" with the activity, as the publish page's 关联活动 panel
//! does (see [`association`]).

use atp_core::text::{from_millis, html_to_text};
use atp_core::{Action, Collection, Draft, Error, Page, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::notes;

const LIST: &str = "/api/galaxy/v2/creator/activity_center/list";
const KEEP: &str = "/api/galaxy/v2/creator/activity_favor";

/// The activity list: all (`type=1`) or the kept ones (`type=2`), in the
/// center's order (`sort=1`) or latest first (`sort=2`).
pub async fn list(api: &Api, kept: bool, latest: bool) -> Result<Vec<Value>> {
  api.require_login()?;
  let kind = if kept { "2" } else { "1" };
  let sort = if kept || latest { "2" } else { "1" };
  let data = api
    .creator_get(
      LIST,
      &[
        ("sort", sort),
        ("type", kind),
        ("source", "3"),
        ("topic_activity", "0"),
      ],
    )
    .await?;
  Ok(data.list("activity_list").to_vec())
}

/// Activities whose name, reward or topics contain `keyword` (case-insensitive).
pub fn matching(rows: Vec<Value>, keyword: Option<&str>) -> Vec<Value> {
  let Some(k) = keyword.map(str::to_lowercase).filter(|k| !k.is_empty()) else {
    return rows;
  };
  rows
    .into_iter()
    .filter(|a| {
      let topics = a.list("topic_infos").iter().filter_map(|t| t.str("name"));
      [a.str("activity_name"), a.str("activity_reward")]
        .into_iter()
        .flatten()
        .chain(topics)
        .any(|s| s.to_lowercase().contains(&k))
    })
    .collect()
}

/// One activity by `#N` of the last list, its numeric activity id, its page
/// id, a detail link, or its exact name.
pub async fn find(api: &Api, arg: &str) -> Result<Value> {
  let arg = api.ctx.collection_ref(arg)?;
  let arg = arg.as_str();
  let page_id = arg
    .split(['?', '#'])
    .next()
    .and_then(|p| p.rsplit('/').next())
    .unwrap_or(arg);
  let hit = |a: &Value| {
    a.str("activity_id").as_deref() == Some(arg)
      || a.str("page_id").as_deref() == Some(page_id)
      || a.str("activity_name").as_deref() == Some(arg)
  };
  for kept in [false, true] {
    if let Some(a) = list(api, kept, false).await?.into_iter().find(hit) {
      return Ok(a);
    }
  }
  Err(Error::not_found(format!(
    "no activity `{arg}` in the activity center; `atp xhs events` lists them"
  )))
}

/// An activity as a collection: its topics are what notes join it with.
pub fn collection(activity: &Value) -> Collection {
  let mut event = Collection {
    id: activity.str("activity_id").unwrap_or_default(),
    kind: "event".into(),
    name: activity.str("activity_name").unwrap_or_default(),
    description: activity.str("activity_reward").map(|r| html_to_text(&r)),
    url: activity.str("activity_link"),
    raw: Some(activity.clone()),
    ..Collection::default()
  };
  let topics: Vec<String> = activity
    .list("topic_infos")
    .iter()
    .filter_map(|t| t.str("name"))
    .collect();
  let time = |k: &str| {
    activity
      .i64(k)
      .and_then(from_millis)
      .map(|t| json!(t.to_string()))
  };
  for (key, value) in [
    ("page_id", activity.str("page_id").map(Value::from)),
    ("start", time("start_time")),
    ("end", time("end_time")),
    ("topics", Some(json!(topics))),
    ("status", activity.i64("activity_status").map(Value::from)),
    ("kept", Some(json!(activity.i64("focus_status") == Some(1)))),
    ("post_link", activity.str("pc_post_link").map(Value::from)),
  ] {
    if let Some(v) = value {
      event.extra.insert(key.into(), v);
    }
  }
  event
}

/// One activity with every field shown (a table would hide the period).
pub fn detail(activity: &Value) -> Value {
  let mut event = collection(activity);
  event.raw = None;
  json!(event)
}

pub fn page(rows: &[Value]) -> Page<Collection> {
  Page::last(rows.iter().map(collection).collect())
}

/// Keep (收藏) an activity, or drop it with `undo`.
pub async fn keep(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let activity = find(api, arg).await?;
  let page_id = activity
    .str("page_id")
    .ok_or_else(|| Error::upstream("the activity has no page id"))?;
  let body = json!({ "type": if undo { "2" } else { "1" }, "target_id": page_id });
  api.creator_post(KEEP, &body).await?;
  let name = activity.str("activity_name").unwrap_or_default();
  Ok(Action::done(if undo { "unkeep" } else { "keep" }, name).with_id(page_id))
}

/// Publish a note that joins an activity.
pub async fn join(api: &Api, arg: &str, draft: &Draft) -> Result<Action> {
  let activity = find(api, arg).await?;
  let event = association(&activity)?;
  let action = notes::publish_note(api, draft, Some(&event)).await?;
  Ok(action.with_message(format!("joined {}", event.name)))
}

/// What a note joining an activity carries, as the publish page builds it
/// from the activity's `pc_post_link`: the activity's topics, the activity
/// center as the note's source, and an `ACTIVITY_COMPONENT` relation for the
/// business binds (one activity per note).
pub struct Association {
  pub topics: Vec<Value>,
  pub relation: Value,
  pub name: String,
}

impl Association {
  /// The note's `source` field.
  pub fn source(&self) -> String {
    let extra = json!({"subType": "web_activity_center", "ad_channel": "ditto", "systemId": "web"});
    json!({"type": "web", "ids": "", "extraInfo": extra.to_string()}).to_string()
  }
}

fn association(activity: &Value) -> Result<Association> {
  let id = activity
    .str("activity_id")
    .ok_or_else(|| Error::upstream("the activity has no id"))?;
  let name = activity.str("activity_name").unwrap_or_default();
  let info = json!({
    "id": id,
    "name": name,
    "start_time": activity.i64("start_time"),
    "end_time": activity.i64("end_time"),
  });
  let relation = json!({
    "type": "ACTIVITY_COMPONENT",
    "relationList": [{
      "bizType": "ACTIVITY_COMPONENT",
      "bizId": id,
      "extraInfo": info.to_string(),
    }],
  });
  let topics = activity
    .list("topic_infos")
    .iter()
    .filter(|t| t.str("id").is_some() && t.str("name").is_some())
    .map(|t| json!({ "id": t.str("id"), "name": t.str("name"), "link": t.str("link"), "type": "topic" }))
    .collect();
  Ok(Association {
    topics,
    relation,
    name,
  })
}

//! `listen`: `/sync` long polling. Each new timeline event of the joined
//! rooms (or of the rooms named) becomes a post; invites too. The sync
//! position is kept in the cache, so the next `listen` picks up what came in
//! between.

use std::collections::HashMap;
use std::time::Duration;

use social_core::http::Method;
use social_core::output::Stream;
use social_core::{Error, Post, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::parse::{self, Members};
use crate::rooms;
use crate::users;

const POLL_MS: u64 = 30_000;
const MAX_FAILURES: u32 = 5;

fn filter(rooms: &[String]) -> String {
  let mut room = json!({
    "timeline": {"limit": 50},
    "state": {"lazy_load_members": true},
    "ephemeral": {"types": []},
    "account_data": {"types": []},
  });
  if !rooms.is_empty() {
    room["rooms"] = json!(rooms);
  }
  json!({"presence": {"types": []}, "account_data": {"types": []}, "room": room}).to_string()
}

pub async fn listen(api: &Api, chats: &[String], out: &Stream) -> Result<()> {
  api.require()?;
  let me = users::my_id(api).await?;
  let mut ids = Vec::new();
  for c in chats {
    ids.push(rooms::id(api, c).await?);
  }
  // Room names, once.
  let mut names: HashMap<String, String> = rooms::list(api)
    .await?
    .items
    .into_iter()
    .map(|c| (c.id, c.name))
    .collect();
  let filter = filter(&ids);
  let key = format!(
    "since-{}",
    me.replace(|c: char| !c.is_ascii_alphanumeric(), "_")
  );
  let mut since: Option<String> = api
    .ctx
    .store
    .cache_get(&key, Duration::from_secs(7 * 24 * 3600));
  let mut failures = 0;
  loop {
    let mut query = vec![("filter", filter.clone())];
    match &since {
      Some(s) => {
        query.push(("since", s.clone()));
        query.push(("timeout", POLL_MS.to_string()));
      }
      // First run: only the position, not the backlog.
      None => query.push(("timeout", "0".into())),
    }
    let token = api.require()?;
    let result = api
      .call_with(
        Some(&token),
        Method::GET,
        "/sync",
        &query,
        None,
        Some(Duration::from_millis(POLL_MS + 30_000)),
      )
      .await;
    let v = match result {
      Ok(v) => {
        failures = 0;
        v
      }
      Err(e) if e.code == social_core::ErrorCode::NotAuthenticated => return Err(e),
      Err(e) => {
        failures += 1;
        if failures >= MAX_FAILURES {
          return Err(e);
        }
        tracing::debug!("sync failed ({e}); retrying");
        tokio::time::sleep(Duration::from_secs(u64::from(failures) * 3)).await;
        continue;
      }
    };
    if since.is_some() {
      emit(&v, &ids, &mut names, out);
    }
    let next = v
      .str("next_batch")
      .ok_or_else(|| Error::upstream("sync answered without next_batch"))?;
    api.ctx.store.cache_put(&key, &next);
    since = Some(next);
  }
}

fn emit(v: &Value, ids: &[String], names: &mut HashMap<String, String>, out: &Stream) {
  let wanted = |rid: &str| ids.is_empty() || ids.iter().any(|i| i == rid);
  if let Some(joined) = v.at("rooms").get("join").and_then(Value::as_object) {
    for (rid, r) in joined {
      if !wanted(rid) {
        continue;
      }
      let events: Vec<Value> = r
        .list("state.events")
        .iter()
        .chain(r.list("timeline.events"))
        .cloned()
        .collect();
      for e in &events {
        if e.str("type").as_deref() == Some("m.room.name")
          && let Some(n) = e.at("content").str("name")
        {
          names.insert(rid.clone(), n);
        }
      }
      let mut members = Members::default();
      members.absorb(&events);
      let name = names.get(rid).cloned().unwrap_or_default();
      for e in r.list("timeline.events") {
        if let Some(post) = parse::event(rid, &name, e, &members) {
          out.post(&post);
        }
      }
    }
  }
  if let Some(invited) = v.at("rooms").get("invite").and_then(Value::as_object) {
    for (rid, _) in invited {
      if !wanted(rid) {
        continue;
      }
      let mut p = Post {
        id: rid.clone(),
        kind: "invite".into(),
        text: Some(format!("invited to {rid}")),
        ..Post::default()
      };
      p.extra.insert("chat".into(), json!(rid));
      out.post(&p);
    }
  }
}

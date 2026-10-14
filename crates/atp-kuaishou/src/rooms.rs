//! Live rooms (`live.kuaishou.com/u/<id>`): the rooms of the live home
//! page, one room's details and stream addresses, its gift panel, and the
//! replays of a user's past streams.

use std::collections::{BTreeMap, HashSet};

use atp_core::{Error, Page, PageReq, Post, Result, Value, ValueExt, json, paging};

use crate::api::Api;
use crate::{parse, refs, users};

const HOME: &str = "/live_api/home/list";
const DETAIL: &str = "/live_api/liveroom/livedetail";
const GIFTS: &str = "/live_api/emoji/gift-list";
const ALL_GIFTS: &str = "/live_api/emoji/allgifts";
const REPLAYS: &str = "/rest/v/live/playBack/list";

/// The rooms of the live home page, live ones first.
pub async fn list(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  let data = api.live(HOME).send().await?;
  let mut seen = HashSet::new();
  let mut rooms: Vec<Post> = parse::home_rooms(&data)
    .into_iter()
    .filter_map(parse::room)
    .filter(|r| seen.insert(r.id.clone()))
    .collect();
  rooms.sort_by_key(|r| r.extra.get("live") != Some(&Value::Bool(true)));
  Ok(paging::slice(rooms, page))
}

/// A room: its details, or its entry on the live home page.
pub async fn room(api: &Api, arg: &str) -> Result<Post> {
  let id = refs::room(arg)?;
  match details(api, &id).await {
    Ok(Some(post)) => return Ok(post),
    Ok(None) => {}
    Err(e) => tracing::debug!("live details of {id}: {e}"),
  }
  let data = api.live(HOME).send().await?;
  parse::home_rooms(&data)
    .into_iter()
    .find(|r| r.str("author.id").as_deref() == Some(id.as_str()))
    .and_then(parse::room)
    .ok_or_else(|| {
      Error::not_found(format!(
        "no Kuaishou live room {id} (not streaming, or not on the live home page)"
      ))
    })
}

async fn details(api: &Api, id: &str) -> Result<Option<Post>> {
  let data = api
    .live(DETAIL)
    .arg("principalId", id)
    .referer(&refs::live_url(id))
    .send()
    .await?;
  Ok(parse::room(&data))
}

/// The id of the stream a room is airing.
pub fn stream_id(room: &Post) -> Result<String> {
  room
    .extra
    .get("live_stream_id")
    .and_then(Value::as_str)
    .map(str::to_owned)
    .ok_or_else(|| Error::not_found(format!("Kuaishou live room {} is not streaming", room.id)))
}

/// The gift panel of a room (names and prices in Kuaishou coins).
pub async fn gifts(api: &Api, arg: &str) -> Result<Value> {
  let room = room(api, arg).await?;
  let data = api
    .live(GIFTS)
    .arg("liveStreamId", stream_id(&room)?)
    .referer(&refs::live_url(&room.id))
    .send()
    .await?;
  Ok(Value::Array(
    data.list("gifts").iter().filter_map(parse::gift).collect(),
  ))
}

/// Gift id → name for a room: its panel plus the full gift dictionary.
pub async fn gift_names(api: &Api, room: &str, stream: &str) -> BTreeMap<String, String> {
  let mut names = BTreeMap::new();
  let referer = refs::live_url(room);
  let panel = api
    .live(GIFTS)
    .arg("liveStreamId", stream)
    .referer(&referer);
  let all = api.live(ALL_GIFTS).referer(&referer);
  for call in [panel, all] {
    let data = match call.send().await {
      Ok(data) => data,
      Err(e) => {
        tracing::debug!("gift names: {e}");
        continue;
      }
    };
    let list = data.list("gifts");
    // The panel lists gifts; the dictionary is keyed by gift id.
    let entries: Vec<(Option<String>, &Value)> = if list.is_empty() {
      let dict = data.as_object().into_iter().flatten();
      dict.map(|(k, g)| (Some(k.clone()), g)).collect()
    } else {
      list.iter().map(|g| (None, g)).collect()
    };
    for (key, gift) in entries {
      if let (Some(id), Some(name)) = (gift.str("id").or(key), gift.str("name")) {
        names.insert(id, name);
      }
    }
  }
  names
}

/// Replays of a user's past live streams.
pub async fn replays(api: &Api, user: &str, page: &PageReq) -> Result<Page<Post>> {
  let eid = users::eid(api, user).await?;
  let v = api
    .www(REPLAYS)
    .json(json!({ "userId": eid, "pcursor": page.cursor.clone().unwrap_or_default() }))
    .referer(&refs::profile_url(&eid))
    .send()
    .await?;
  let posts: Vec<Post> = ["list", "playbackList", "feeds", "data.list"]
    .iter()
    .flat_map(|k| v.list(k))
    .filter_map(|r| parse::replay(r, &eid))
    .collect();
  let next = parse::more(v.first_str(&["pcursor", "data.pcursor"])).filter(|_| !posts.is_empty());
  Ok(Page::new(posts, next))
}

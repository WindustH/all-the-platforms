//! Live rooms (from the live home list or a room's details), replays and
//! gifts. A room is keyed by its anchor's live id, the `<id>` of
//! `live.kuaishou.com/u/<id>`; the current stream's id is
//! `extra.live_stream_id`.

use atp_core::text::from_millis;
use atp_core::{Extra, Post, User, UserStats, Value, ValueExt, json};

use super::put;
use crate::refs::live_url;

/// A room: `{id, caption, poster, playUrls, author, gameInfo, ...}`, or a
/// details answer holding that as `liveStream` beside `author`.
pub fn room(v: &Value) -> Option<Post> {
  let stream = if v.at("liveStream").is_object() {
    v.at("liveStream")
  } else {
    v
  };
  let anchor = [v.at("author"), stream.at("author")]
    .into_iter()
    .find(|a| a.is_object())?;
  let id = anchor.first_str(&["id", "principalId"])?;
  Some(Post {
    kind: "live".into(),
    title: stream.first_str(&["caption", "title"]),
    url: Some(live_url(&id)),
    author: Some(anchor_user(anchor, &id)),
    created_at: ["startTime", "statrtTime"]
      .iter()
      .find_map(|k| stream.i64(k))
      .and_then(from_millis),
    extra: room_extra(v, stream, anchor),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  })
}

fn anchor_user(anchor: &Value, id: &str) -> User {
  User {
    url: Some(live_url(id)),
    id: id.to_owned(),
    name: anchor.str("name").unwrap_or_default(),
    avatar: anchor.first_str(&["avatar", "headUrl"]),
    bio: anchor.str("description"),
    stats: UserStats {
      followers: anchor.count("counts.fan"),
      following: anchor.count("counts.follow"),
      ..UserStats::default()
    },
    ..User::default()
  }
}

/// Live state, stream id, audience, cover, category and stream addresses.
/// A room with addresses is live, whatever its flags say.
fn room_extra(v: &Value, stream: &Value, anchor: &Value) -> Extra {
  let streams = streams(stream);
  let flags = [
    v.bool("isLiving"),
    stream.bool("living"),
    anchor.bool("living"),
  ];
  let living = !streams.is_empty() || flags.into_iter().flatten().any(|l| l);
  let mut extra = Extra::new();
  extra.insert("live".into(), living.into());
  put(
    &mut extra,
    "live_stream_id",
    stream.str("id").map(Value::from),
  );
  let viewers = stream.first_str(&["watchingCount", "displayWatchingCount"]);
  put(&mut extra, "viewers", viewers.map(Value::from));
  put(&mut extra, "cover", stream.str("poster").map(Value::from));
  let category = [v, stream].iter().find_map(|x| x.str("gameInfo.name"));
  put(&mut extra, "category", category.map(Value::from));
  if !streams.is_empty() {
    extra.insert("streams".into(), Value::Array(streams));
  }
  extra
}

/// The pull addresses of a room, `{name, url}` per quality (and `codec`
/// when the room lists several): `playUrls` is a list on the home page and
/// a map by codec in the details, which add an HLS address.
fn streams(stream: &Value) -> Vec<Value> {
  let sets: Vec<(Option<&String>, &Value)> = match stream.at("playUrls") {
    Value::Array(list) => list.iter().map(|p| (None, p)).collect(),
    Value::Object(map) => map.iter().map(|(codec, p)| (Some(codec), p)).collect(),
    _ => Vec::new(),
  };
  let mut out: Vec<Value> = sets
    .into_iter()
    .flat_map(|(codec, p)| {
      p.list("adaptationSet.representation")
        .iter()
        .map(move |r| (codec, r))
    })
    .filter_map(|(codec, r)| {
      let mut s = json!({ "name": r.str("name")?, "url": r.str("url")? });
      if let Some(codec) = codec {
        s["codec"] = json!(codec);
      }
      Some(s)
    })
    .collect();
  if let Some(hls) = stream.str("hlsPlayUrl") {
    out.push(json!({ "name": "hls", "url": hls }));
  }
  out
}

/// The anchor ids and stream ids of the live home list.
pub fn home_rooms(data: &Value) -> Vec<&Value> {
  data
    .list("list")
    .iter()
    .flat_map(|section| section.list("gameLiveInfo"))
    .flat_map(|group| group.list("liveInfo"))
    .filter(|item| item.str("id").is_some() && item.str("author.id").is_some())
    .collect()
}

/// A replay of a past live stream.
pub fn replay(v: &Value, anchor: &str) -> Option<Post> {
  let id = v.first_str(&["id", "productId", "photoId"])?;
  Some(Post {
    url: Some(format!("https://live.kuaishou.com/playback/{id}")),
    kind: "replay".into(),
    title: v.first_str(&["caption", "title"]),
    author: Some(User {
      url: Some(live_url(anchor)),
      id: anchor.to_owned(),
      ..User::default()
    }),
    created_at: v
      .first_str(&["createTime", "timestamp"])
      .and_then(|t| t.parse().ok())
      .and_then(from_millis),
    metrics: atp_core::Metrics {
      views: v.count("viewCount"),
      likes: v.count("likeCount"),
      comments: v.count("commentCount"),
      ..Default::default()
    },
    raw: Some(v.clone()),
    id,
    ..Post::default()
  })
}

/// A gift of a room's panel: id, name and price in Kuaishou coins.
pub fn gift(v: &Value) -> Option<Value> {
  Some(json!({
    "id": v.str("id")?,
    "name": v.str("name").unwrap_or_default(),
    "price_coins": v.u64("unitPrice"),
  }))
}

//! A live room to follow: `/api-live/user/room/` turns a host's handle
//! into the room and whether it is live; a room id is taken as it is.

use atp_core::{Error, Result, ValueExt};

use crate::api::Api;
use crate::refs::{self, LiveRef};

const USER_ROOM: &str = "/api-live/user/room/";
/// `liveRoom.status` of a room that is on air.
const LIVE: i64 = 2;

pub struct Room {
  pub id: String,
  /// How the room is named in chat ids: `@handle`, or the room id.
  pub key: String,
  pub title: String,
}

pub async fn find(api: &Api, arg: &str) -> Result<Room> {
  match refs::live(arg)? {
    LiveRef::Room(id) => Ok(Room {
      key: id.clone(),
      id,
      title: String::new(),
    }),
    LiveRef::Handle(handle) => by_handle(api, &handle).await,
  }
}

async fn by_handle(api: &Api, handle: &str) -> Result<Room> {
  let params = [
    ("aid", "1988"),
    ("app_language", "en"),
    ("app_name", "tiktok_web"),
    ("device_platform", "web_pc"),
    ("sourceType", "54"),
    ("uniqueId", handle),
  ];
  let v = api
    .unsigned(USER_ROOM, &params, &refs::live_url(handle))
    .await?;
  let room = v.at("data.liveRoom");
  let id = v
    .str("data.user.roomId")
    .filter(|id| !id.is_empty() && id != "0");
  match id {
    Some(id) if room.i64("status") == Some(LIVE) => Ok(Room {
      id,
      key: format!("@{handle}"),
      title: room.str("title").unwrap_or_default(),
    }),
    _ => Err(Error::not_found(format!("@{handle} is not live now"))),
  }
}

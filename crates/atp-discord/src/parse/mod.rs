//! Discord payloads → the shared model: guilds and channels are
//! collections, messages are posts (`CHANNEL/MESSAGE` ids, permalinks where
//! the guild is known), users are users.

mod chats;
mod messages;
mod users;

use crate::api::WEB;
use atp_core::text::from_millis;

pub use self::chats::*;
pub use self::messages::*;
pub use self::users::*;

const CDN: &str = "https://cdn.discordapp.com";

/// Milliseconds of 2015-01-01, where snowflake time starts.
const EPOCH: i64 = 1_420_070_400_000;

/// When a snowflake id was made.
pub fn snowflake_time(id: &str) -> Option<jiff::Timestamp> {
  let n: i64 = id.parse().ok()?;
  from_millis((n >> 22) + EPOCH)
}

/// The smallest snowflake of a moment (for `after=` / `min_id=`).
pub fn snowflake_at(time: jiff::Timestamp) -> String {
  ((time.as_millisecond() - EPOCH).max(0) << 22).to_string()
}

pub fn permalink(guild: &str, channel: &str, message: Option<&str>) -> String {
  match message {
    Some(m) => format!("{WEB}/channels/{guild}/{channel}/{m}"),
    None => format!("{WEB}/channels/{guild}/{channel}"),
  }
}

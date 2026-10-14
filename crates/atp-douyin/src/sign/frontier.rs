//! The `signature` of the webcast websocket: `frontierSign`
//! ([`atp_bytedance::webcast::frontier`]) over `X-MS-STUB`, the MD5 of a few
//! connection parameters.

use atp_bytedance::webcast::frontier;
use md5::{Digest, Md5};

/// The `webcast_sdk_version` the stub and the connection name.
pub const SDK_VERSION: &str = "1.0.14-beta.0";

/// The signature for a connection to `room_id` as `user_unique_id`.
pub fn signature(room_id: &str, user_unique_id: &str) -> String {
  let stub = format!(
    "live_id=1,aid=6383,version_code=180800,webcast_sdk_version={SDK_VERSION},\
     room_id={room_id},sub_room_id=,sub_channel_id=,did_rule=3,\
     user_unique_id={user_unique_id},device_platform=web,device_type=,ac=,\
     identity=audience"
  );
  frontier::sign(&Md5::digest(stub.as_bytes()))
}

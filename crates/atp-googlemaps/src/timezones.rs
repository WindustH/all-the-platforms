//! Time Zone API: the zone of a point at a moment, and its local time.

use atp_core::text::from_secs;
use atp_core::{Result, Value, ValueExt, json};

use crate::api::Api;
use crate::refs;

pub async fn timezone(api: &Api, location: &str, at: Option<i64>) -> Result<Value> {
  let point = refs::expect_point(location, api.system())?;
  let at = at.unwrap_or_else(|| jiff::Timestamp::now().as_second());
  let params = [("location", point.lat_lng()), ("timestamp", at.to_string())];
  let v = api.service("/timezone/json", &params).await?;
  if v.str("status").as_deref() == Some("ZERO_RESULTS") {
    return Err(atp_core::Error::not_found(format!(
      "Google has no time zone at {location}"
    )));
  }
  let raw = v.i64("rawOffset").unwrap_or(0);
  let dst = v.i64("dstOffset").unwrap_or(0);
  let local = from_secs(at + raw + dst).map(|t| t.strftime("%Y-%m-%d %H:%M:%S").to_string());
  Ok(json!({
    "location": point,
    "time_zone": v.str("timeZoneId"),
    "name": v.str("timeZoneName"),
    "utc_offset_s": raw + dst,
    "dst_offset_s": dst,
    "at": from_secs(at),
    "local_time": local,
  }))
}

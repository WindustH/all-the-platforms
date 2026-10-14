//! Weather (`/v3/weather/weatherInfo`): live conditions or a four-day
//! forecast of a city or district.

use atp_core::{Result, Value, ValueExt, json};

use crate::api::Api;
use crate::districts;

pub async fn weather(api: &Api, region: &str, forecast: bool) -> Result<Value> {
  let adcode = districts::adcode(api, region).await?;
  let params = [
    ("city", adcode),
    (
      "extensions",
      if forecast { "all" } else { "base" }.to_owned(),
    ),
  ];
  let v = api.get("/v3/weather/weatherInfo", &params).await?;
  if forecast {
    return Ok(forecasts(v.at("forecasts.0")));
  }
  let l = v.at("lives.0");
  Ok(json!({
    "province": l.str("province"),
    "city": l.str("city"),
    "adcode": l.str("adcode"),
    "weather": l.str("weather"),
    "temperature_c": l.i64("temperature"),
    "humidity_pct": l.i64("humidity"),
    "wind_direction": l.str("winddirection"),
    "wind_power": l.str("windpower"),
    "reported_at": l.str("reporttime"),
  }))
}

fn forecasts(f: &Value) -> Value {
  let days: Vec<Value> = f
    .list("casts")
    .iter()
    .map(|c| {
      json!({
        "date": c.str("date"),
        "week": c.str("week"),
        "day": c.str("dayweather"),
        "night": c.str("nightweather"),
        "high_c": c.i64("daytemp"),
        "low_c": c.i64("nighttemp"),
        "day_wind": wind(c, "day"),
        "night_wind": wind(c, "night"),
      })
    })
    .collect();
  json!({
    "province": f.str("province"),
    "city": f.str("city"),
    "adcode": f.str("adcode"),
    "reported_at": f.str("reporttime"),
    "days": days,
  })
}

fn wind(c: &Value, part: &str) -> Option<String> {
  let direction = c.str(&format!("{part}wind"))?;
  let power = c.str(&format!("{part}power")).unwrap_or_default();
  Some(format!("{direction} {power}").trim_end().to_owned())
}

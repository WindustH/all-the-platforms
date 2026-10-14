//! Weather (`/ws/weather/v1/`): the weather now, or the forecast for the
//! next week, of a district (adcode), a point or a place name.

use atp_core::{Result, Value, ValueExt, json};

use crate::api::Api;
use crate::{districts, refs};

pub async fn weather(api: &Api, region: &str, forecast: bool) -> Result<Value> {
  let mut params = vec![("type", if forecast { "future" } else { "now" }.to_owned())];
  if forecast {
    params.push(("get_md", "1".to_owned()));
  }
  match refs::point(region, api.system()) {
    Some(point) => params.push(("location", point.lat_lng())),
    None => params.push(("adcode", districts::adcode(api, region).await?)),
  }
  let v = api.get("/ws/weather/v1/", &params).await?;
  let r = v.at("result");
  let place = if forecast {
    r.at("forecast.0")
  } else {
    r.at("realtime.0")
  };
  let mut out = json!({
    "province": place.str("province"),
    "city": place.str("city"),
    "district": place.str("district"),
    "adcode": place.str("adcode"),
    "reported_at": place.str("update_time"),
  });
  let details = if forecast {
    days(place)
  } else {
    now(place.at("infos"))
  };
  if let (Value::Object(out), Value::Object(details)) = (&mut out, details) {
    out.extend(details);
  }
  Ok(out)
}

fn now(i: &Value) -> Value {
  json!({
    "weather": i.str("weather"),
    "temperature_c": i.f64("temperature"),
    "humidity_pct": i.f64("humidity"),
    "wind_direction": i.str("wind_direction"),
    "wind_power": i.str("wind_power"),
    "pressure_hpa": i.f64("air_pressure"),
  })
}

fn days(f: &Value) -> Value {
  let days: Vec<Value> = f
    .list("infos")
    .iter()
    .map(|d| {
      json!({
        "date": d.str("date"),
        "week": d.str("week"),
        "day": d.str("day.weather"),
        "night": d.str("night.weather"),
        "high_c": d.f64("day.temperature"),
        "low_c": d.f64("night.temperature"),
        "day_wind": wind(d.at("day")),
        "night_wind": wind(d.at("night")),
      })
    })
    .collect();
  json!({ "days": days })
}

fn wind(part: &Value) -> Option<String> {
  let direction = part.str("wind_direction")?;
  let power = part.str("wind_power").unwrap_or_default();
  Some(format!("{direction} {power}").trim_end().to_owned())
}

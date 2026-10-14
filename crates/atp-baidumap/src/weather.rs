//! Weather v1 (`/weather/v1/`): the weather now, or the forecast for the
//! next days, of a district (adcode), a point or a place name.

use atp_core::{Result, Value, ValueExt, json};

use crate::api::Api;
use crate::{geocoding, refs};

pub async fn weather(api: &Api, region: &str, forecast: bool) -> Result<Value> {
  let region = region.trim();
  let wire = api.wire();
  let mut params = vec![("data_type", if forecast { "fc" } else { "now" }.to_owned())];
  if region.len() == 6 && region.bytes().all(|b| b.is_ascii_digit()) {
    params.push(("district_id", region.to_owned()));
  } else {
    let end = geocoding::end(api, region, None).await?;
    params.push(("location", end.location.lng_lat()));
    params.push(("coordtype", refs::short(wire).to_owned()));
  }
  let v = api.get("/weather/v1/", &params).await?;
  let r = v.at("result");
  let place = r.at("location");
  let place = if place.is_null() {
    r.at("address")
  } else {
    place
  };
  let mut out = json!({
    "province": place.str("province"),
    "city": place.str("city"),
    "district": place.str("name"),
    "adcode": place.str("id"),
  });
  let details = if forecast { days(r) } else { now(r.at("now")) };
  if let (Value::Object(out), Value::Object(details)) = (&mut out, details) {
    out.extend(details);
  }
  Ok(out)
}

fn now(n: &Value) -> Value {
  json!({
    "weather": n.str("text"),
    "temperature_c": n.f64("temp"),
    "feels_like_c": n.f64("feels_like"),
    "humidity_pct": n.f64("rh"),
    "wind_direction": n.str("wind_dir"),
    "wind_power": n.str("wind_class"),
    "aqi": n.f64("aqi").filter(|a| *a < 999_999.0),
    "reported_at": n.str("uptime"),
  })
}

fn days(r: &Value) -> Value {
  let days: Vec<Value> = r
    .list("forecasts")
    .iter()
    .map(|f| {
      json!({
        "date": f.str("date"),
        "week": f.str("week"),
        "day": f.str("text_day"),
        "night": f.str("text_night"),
        "high_c": f.f64("high"),
        "low_c": f.f64("low"),
        "day_wind": wind(f, "day"),
        "night_wind": wind(f, "night"),
      })
    })
    .collect();
  json!({ "days": days })
}

fn wind(f: &Value, part: &str) -> Option<String> {
  let direction = f.str(&format!("wd_{part}"))?;
  let power = f.str(&format!("wc_{part}")).unwrap_or_default();
  Some(format!("{direction} {power}").trim_end().to_owned())
}

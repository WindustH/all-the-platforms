//! Flight fares: the fare calendar of the H5 flight search
//! (`15380/flightIntlAndInlandLowestPriceSearch`, the lowest one-way fare
//! of each day between two cities) and airport cities by name
//! (`flights.ctrip.com/international/search/api/poi/search`).
//!
//! Flight lists stay closed: the gateway refuses the H5 list search to web
//! clients (11010), the streamed search sits behind the firewall's bot
//! check, and `flights.ctrip.com` wants a login and a browser fingerprint.

use std::time::Duration;

use atp_core::text::encode_uri_component;
use atp_core::{Error, Page, PageReq, Post, Result, Value, ValueExt, json, paging};
use jiff::civil::Date;

use crate::api::{Api, FLIGHTS, Site};
use crate::parse;

const TTL: Duration = Duration::from_secs(90 * 24 * 3600);

/// A one-way trip between two cities (names or airport-city codes).
pub struct Route<'a> {
  pub from: &'a str,
  pub to: &'a str,
  /// Only nonstop flights.
  pub direct: bool,
}

/// `fares FROM TO`: the lowest fare of each day from `start` on, as far
/// as Ctrip sells (about a year).
pub async fn fares(
  api: &Api,
  route: &Route<'_>,
  start: Date,
  page: &PageReq,
) -> Result<Page<Post>> {
  let from = code(api, route.from).await?;
  let to = code(api, route.to).await?;
  let selections = if route.direct {
    json!([{ "SelectionType": 1, "SelectionContent": ["1"] }])
  } else {
    json!([])
  };
  let body = json!({
    "SearchType": 1,
    "PassengerList": [{ "PassengerType": "Adult", "PassengerCount": 1 }],
    "DepartNewCityCode": from, "ArriveNewCityCode": to, "Grade": 1,
    "StartDate": format!("{start} 00:00:00"),
    "CalendarSelections": selections,
    "ExtendInfo": []
  });
  let v = api
    .soa(
      "15380/flightIntlAndInlandLowestPriceSearch",
      Site::Mobile,
      body,
    )
    .await?;
  let title = format!("{} → {}", route.from, route.to);
  let first = start.to_string();
  let items: Vec<Post> = v
    .list("PriceList")
    .iter()
    .filter(|p| p.str("DepartDate").is_some_and(|d| *d >= *first))
    .filter_map(|p| parse::fare(p, &from, &to, &title))
    .collect();
  Ok(paging::slice(items, page))
}

/// The airport-city code (`SHA`) of a city, or the code itself; names are
/// looked up once and remembered.
async fn code(api: &Api, arg: &str) -> Result<String> {
  let name = arg.trim();
  if name.len() == 3 && name.bytes().all(|b| b.is_ascii_alphabetic()) {
    return Ok(name.to_ascii_uppercase());
  }
  let key = format!("ctrip-airport-city-{name}");
  if let Some(code) = api.ctx.store.cache_get::<String>(&key, TTL) {
    return Ok(code);
  }
  let url = format!(
    "{FLIGHTS}/international/search/api/poi/search?key={}&mode=0&tripType=1",
    encode_uri_component(name)
  );
  let v = api.get(&url, Site::Flights).await?;
  let code = city_code(v.list("Data"), name)
    .ok_or_else(|| Error::not_found(format!("Ctrip knows no airport city {name}")))?;
  api.ctx.store.cache_put(&key, &code);
  Ok(code)
}

/// The city named `name` (type 5), else the first city, else the city of
/// the first airport.
fn city_code(places: &[Value], name: &str) -> Option<String> {
  let city = |p: &&Value| p.i64("Type") == Some(5);
  let bare = name.trim_end_matches('市');
  places
    .iter()
    .filter(city)
    .find(|p| p.str("Name").as_deref() == Some(bare))
    .or_else(|| places.iter().find(city))
    .and_then(|p| p.str("Code"))
    .or_else(|| places.first()?.str("Datas.0.Code"))
}

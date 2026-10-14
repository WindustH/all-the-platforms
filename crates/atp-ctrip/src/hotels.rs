//! Hotels: a city's list page and the hotel page on `hotels.ctrip.com`
//! (both open to visitors, without prices), and the rooms of a stay
//! (`33278/json/getHotelRoomListInland`), which Ctrip shows after a login.
//!
//! The list page shows its first screen of hotels; further pages and
//! prices come from a hotel-list service that wants Ctrip's anti-spider
//! token, which atp does not make.

use atp_core::{Error, Page, Post, Result, Value, ValueExt, json};
use jiff::ToSpan;
use jiff::civil::Date;

use crate::api::{Api, HOTELS, Site};
use crate::parse;
use crate::refs::{self, day, today};

/// A stay: check-in and check-out days (`YYYY-MM-DD`).
#[derive(Debug, Clone)]
pub struct Stay {
  pub checkin: Date,
  pub checkout: Date,
}

impl Stay {
  /// From the given days: check-in defaults to today in Beijing,
  /// check-out to the day after check-in.
  pub fn new(checkin: Option<&str>, checkout: Option<&str>) -> Result<Self> {
    let checkin = match checkin {
      Some(d) => day(d)?,
      None => today(),
    };
    let checkout = match checkout {
      Some(d) => day(d)?,
      None => checkin
        .checked_add(1.day())
        .map_err(|e| Error::input(e.to_string()))?,
    };
    if checkout <= checkin {
      return Err(Error::input("--checkout must come after --checkin"));
    }
    Ok(Self { checkin, checkout })
  }
}

/// `hotels CITY`: the city's list page (`city` is a hotel-side city id).
pub async fn list(api: &Api, city: &str, stay: &Stay) -> Result<Page<Post>> {
  let url = format!(
    "{HOTELS}/hotels/list?city={city}&checkin={}&checkout={}",
    stay.checkin, stay.checkout
  );
  let html = api.page(&url, Site::Hotels).await?;
  let data = parse::rsc(&html, "initListData")
    .ok_or_else(|| Error::upstream("no hotels in Ctrip's list page"))?;
  let items = data
    .list("hotelList")
    .iter()
    .filter_map(parse::hotel_item)
    .collect();
  Ok(Page::last(items))
}

/// `hotel ID` and `read`: the hotel page.
pub async fn read(api: &Api, id: &str) -> Result<Post> {
  let html = api.page(&refs::hotel_url(id), Site::Hotels).await?;
  parse::rsc(&html, "hotelDetailResponse")
    .and_then(|d| parse::hotel(&d, id))
    .ok_or_else(|| Error::not_found(format!("Ctrip shows no hotel {id}")))
}

/// `rooms HOTEL`: room plans and prices of a stay (login).
pub async fn rooms(api: &Api, id: &str, stay: &Stay, adults: u8) -> Result<Page<Post>> {
  api.require_login()?;
  let hotel = id
    .parse::<u64>()
    .map_err(|_| Error::input(format!("not a hotel id: {id}")))?;
  let compact = |d: Date| d.strftime("%Y%m%d").to_string();
  let mut head = api.hotel_head("102003");
  head["cver"] = "hotels".into();
  head["syscode"] = "09".into();
  let body = json!({
    "search": {
      "checkIn": compact(stay.checkin), "checkOut": compact(stay.checkout),
      "hotelId": hotel, "subHotelId": hotel, "roomId": 0, "fixSubhotel": 0,
      "priceType": 2, "adult": adults.max(1), "childInfoItems": [], "roomQuantity": 1,
      "mustShowRoomList": [], "cancelPolicyType": 0, "isFirstEnterDetailPage": "T",
      "extras": {}, "filters": [], "isUserSelectCheckInOut": "T", "hotelUniqueKey": "",
      "hasAidInUrl": false, "residenceCode": "CN"
    },
    "genk": true,
    "head": head,
    "ServerData": ""
  });
  let v = api
    .soa("33278/json/getHotelRoomListInland", Site::Hotels, body)
    .await?;
  Ok(Page::last(room_list(v.at("data"), id)))
}

/// Room plans, cheapest first.
fn room_list(data: &Value, hotel: &str) -> Vec<Post> {
  let physical = data.at("physicRoomMap");
  let mut rooms: Vec<Post> = data
    .get("saleRoomMap")
    .and_then(Value::as_object)
    .into_iter()
    .flat_map(|m| m.values())
    .filter_map(|sale| {
      let room = sale
        .str("physicalRoomId")
        .and_then(|id| physical_room(physical, &id))
        .cloned()
        .unwrap_or(Value::Null);
      parse::room(sale, &room, hotel)
    })
    .collect();
  let price = |p: &Post| p.price.as_ref().map_or(f64::MAX, |p| p.amount);
  rooms.sort_by(|a, b| price(a).total_cmp(&price(b)));
  rooms
}

/// The room of `physicRoomMap` with this id (by key, else by its `id`).
fn physical_room<'a>(map: &'a Value, id: &str) -> Option<&'a Value> {
  map.get(id).or_else(|| {
    map
      .as_object()?
      .values()
      .find(|r| r.str("id").as_deref() == Some(id))
  })
}

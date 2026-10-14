//! Trains as posts: one per train of the list page (schedule, seats,
//! fares, tickets left), one per plan with a change (its legs in
//! `extra.legs`). The departure is the post's time.

use atp_core::text::from_beijing;
use atp_core::{Post, Price, Value, ValueExt, json};
use jiff::civil::Date;

use super::{copy, put};

/// A train of the list page (`trainSearchInfo.trainInfoList`) on `date`.
pub fn train(v: &Value, date: &str) -> Option<Post> {
  let number = v.str("trainNumber")?;
  let field = |key: &str| v.str(key).unwrap_or_default();
  let (from, to) = (field("departureStationName"), field("arrivalStationName"));
  let (depart, arrive) = (field("departureTime"), field("arrivalTime"));
  let days = v.u64("takeDays").filter(|d| *d > 0);
  let next_day = days.map(|d| format!("+{d}")).unwrap_or_default();
  let seats = seats(
    v.list("seatItemInfoList"),
    ["seatName", "seatPrice", "seatInventory"],
  );
  let mut post = Post {
    kind: "train".into(),
    title: Some(format!(
      "{number} {from} {depart} → {to} {arrive}{next_day}"
    )),
    text: Some(seat_line(v.str("duration"), &seats)),
    created_at: from_beijing(&format!("{date} {depart}")),
    price: lowest(&seats).map(|p| Price {
      label: Some("起".into()),
      ..Price::cny(p)
    }),
    raw: Some(v.clone()),
    id: number,
    ..Post::default()
  };
  let extra = &mut post.extra;
  copy(extra, v, TRAIN_FIELDS);
  put(extra, "days", days.map(Value::from));
  if v.bool("isSaleStop") == Some(true) {
    put(extra, "sale_stopped", Some(true.into()));
  }
  put(extra, "seats", Some(seats.into()));
  Some(post)
}

/// Extra fields of a train: (name in `extra`, path in the entry).
const TRAIN_FIELDS: &[(&str, &str)] = &[
  ("train_no", "trainNo"),
  ("from", "departureStationName"),
  ("to", "arrivalStationName"),
  ("depart", "departureTime"),
  ("arrive", "arrivalTime"),
  ("minutes", "runTime"),
  ("bookable", "isBookable"),
];

/// Seats as `{name, price, left}` from a list using the given field names.
fn seats(list: &[Value], [name, price, left]: [&str; 3]) -> Vec<Value> {
  list
    .iter()
    .filter_map(|s| {
      Some(json!({
        "name": s.str(name)?,
        "price": s.f64(price).or_else(|| s.f64("Price")).or_else(|| s.f64("price")),
        "left": s.i64(left),
      }))
    })
    .collect()
}

/// `5时47分 · 二等座 ¥576 余99 · 一等座 ¥967 余10`.
fn seat_line(duration: Option<String>, seats: &[Value]) -> String {
  let mut parts: Vec<String> = duration.into_iter().collect();
  parts.extend(seats.iter().map(|s| {
    let name = s.str("name").unwrap_or_default();
    let price = s.f64("price").map(|p| format!(" ¥{p}")).unwrap_or_default();
    let left = match s.i64("left") {
      Some(0) => " 无票".to_owned(),
      Some(n) => format!(" 余{n}"),
      None => String::new(),
    };
    format!("{name}{price}{left}")
  }));
  parts.join(" · ")
}

fn lowest(seats: &[Value]) -> Option<f64> {
  seats
    .iter()
    .filter_map(|s| s.f64("price"))
    .filter(|p| *p > 0.0)
    .reduce(f64::min)
}

/// A plan with one change (`TransferLines` entry): two legs, by train or
/// plane, and the plan's lowest total fare.
pub fn transfer(v: &Value) -> Option<Post> {
  let id = v.str("TripId")?;
  let legs: Vec<Value> = v.list("Lines").iter().filter_map(leg).collect();
  let depart = legs.first()?.str("depart")?;
  let first_day = depart.get(..10).and_then(|d| d.parse::<Date>().ok());
  let title: Vec<String> = legs.iter().map(|l| leg_title(l, first_day)).collect();
  let summary: Vec<String> = [
    v.u64("TotalMinutes").map(|m| format!("全程 {}", hours(m))),
    v.u64("TotalTransferMinutes")
      .map(|m| format!("换乘 {}", hours(m))),
    v.str("TransferLineTag"),
  ]
  .into_iter()
  .flatten()
  .collect();
  let mut lines = vec![summary.join(" · ")];
  lines.extend(legs.iter().map(leg_fares));
  let mut post = Post {
    kind: "transfer".into(),
    title: Some(title.join(" · ")),
    text: Some(lines.join("\n")),
    created_at: from_beijing(&depart),
    price: v.f64("TotalPrice").filter(|p| *p > 0.0).map(|p| Price {
      label: Some("起".into()),
      ..Price::cny(p)
    }),
    raw: Some(v.clone()),
    id,
    ..Post::default()
  };
  let extra = &mut post.extra;
  copy(extra, v, TRANSFER_FIELDS);
  put(extra, "legs", Some(legs.into()));
  Some(post)
}

/// Extra fields of a plan: (name in `extra`, path in the entry).
const TRANSFER_FIELDS: &[(&str, &str)] = &[
  ("plan", "ActionType"),
  ("tag", "TransferLineTag"),
  ("minutes", "TotalMinutes"),
  ("transfer_minutes", "TotalTransferMinutes"),
];

/// A leg of a plan: a train with its seats, or a flight with its fare.
fn leg(line: &Value) -> Option<Value> {
  let time = |v: &Value, key: &str| v.str(key).and_then(|t| clock(&t));
  if let Some(t) = line.get("Train").filter(|t| t.is_object()) {
    let seats = seats(
      t.list("SeatList"),
      ["SeatName", "SeatPrice", "SeatInventory"],
    );
    return Some(json!({
      "kind": "train", "number": t.str("TrainNumber")?,
      "from": t.str("DepartStation"), "to": t.str("ArriveStation"),
      "depart": time(t, "DepartTime")?, "arrive": time(t, "ArriveTime"),
      "minutes": t.u64("RunTime"), "seats": seats,
    }));
  }
  let f = line.at("Flight");
  let airport = |side: &str| {
    let name = f.str(&format!("{side}AirportName"))?;
    let terminal = f.str(&format!("{side}Terminal")).unwrap_or_default();
    Some(format!("{name}{terminal}"))
  };
  Some(json!({
    "kind": "flight", "number": f.str("FlightNumber")?, "airline": f.str("AirlineName"),
    "from": airport("Depart"), "to": airport("Arrive"),
    "depart": time(f, "DepartTime")?, "arrive": time(f, "ArriveTime"),
    "price": f.f64("Price"),
  }))
}

/// `20261020193400` as `2026-10-20 19:34`.
fn clock(compact: &str) -> Option<String> {
  let c = compact
    .get(..12)
    .filter(|c| c.bytes().all(|b| b.is_ascii_digit()))?;
  Some(format!(
    "{}-{}-{} {}:{}",
    &c[..4],
    &c[4..6],
    &c[6..8],
    &c[8..10],
    &c[10..12]
  ))
}

/// `G4 上海 07:00 → 济南西 10:10`, a later day than the plan's first as `+N`.
fn leg_title(leg: &Value, first_day: Option<Date>) -> String {
  let at = |key: &str| {
    let time = leg.str(key).unwrap_or_default();
    let (day, clock) = time.split_once(' ').unwrap_or(("", time.as_str()));
    let later = day
      .parse::<Date>()
      .ok()
      .zip(first_day)
      .map(|(d, first)| (d - first).get_days())
      .filter(|n| *n > 0)
      .map(|n| format!("+{n}"))
      .unwrap_or_default();
    format!("{clock}{later}")
  };
  let field = |key: &str| leg.str(key).unwrap_or_default();
  format!(
    "{} {} {} → {} {}",
    field("number"),
    field("from"),
    at("depart"),
    field("to"),
    at("arrive")
  )
}

/// `G4: 二等座 ¥339 余99 · 一等座 ¥…`, or `CZ8804 南航 ¥498`.
fn leg_fares(leg: &Value) -> String {
  let number = leg.str("number").unwrap_or_default();
  if leg.str("kind").as_deref() == Some("flight") {
    let airline = leg.str("airline").unwrap_or_default();
    let price = leg
      .f64("price")
      .map(|p| format!(" ¥{p}"))
      .unwrap_or_default();
    return format!("{number} {airline}{price}");
  }
  format!("{number}: {}", seat_line(None, leg.list("seats")))
}

/// `13时5分`.
fn hours(minutes: u64) -> String {
  format!("{}时{}分", minutes / 60, minutes % 60)
}

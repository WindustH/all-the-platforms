//! References: links of the sites, prefixed ids (`sight:49894`,
//! `hotel:445394`, `note:15103061`, `ranking:100001406683`, `poi:82922`)
//! and user ids (`clientAuth`).
//!
//! A sight has three ids: its sight id (in `you.ctrip.com/sight/<slug><district>/<sight>.html`),
//! the city's district id and a POI id, which reviews and details want.
//! atp's own sight links are the mobile site's, which carry all three:
//! `m.ctrip.com/webapp/you/gspoi/sight/<district>/<sight>.html?poiId=<poi>`.

use std::sync::LazyLock;

use atp_core::{Error, Result};
use jiff::civil::Date;
use jiff::tz::{Offset, TimeZone};
use regex::Regex;

use crate::api::{FLIGHTS, HOTELS, M, YOU};

macro_rules! re {
  ($name:ident, $pattern:literal) => {
    static $name: LazyLock<Regex> = LazyLock::new(|| Regex::new($pattern).expect("valid"));
  };
}

re!(
  SIGHT_YOU,
  r"you\.ctrip\.com/sight/[^/?#]*?(\d+)/(\d+)\.html"
);
re!(
  SIGHT_M,
  r"ctrip\.com/webapp/you/gspoi/sight/(\d+)/(\d+)\.html"
);
re!(POI, r"[?&]poiId=(\d+)");
re!(
  HOTEL,
  r"(?:hotels\.ctrip\.com/hotels?/(\d+)\.html|hoteldetail/(\d+)\.html|[?&]hotel[iI]d=(\d+))"
);
re!(NOTE, r"[?&]articleId=(\d+)");
re!(TRAVEL, r"you\.ctrip\.com/travels/([^/?#]+)/(\d+)\.html");
re!(RANKING, r"crankingCity/(\d+)\.html");
re!(PREFIXED, r"^(sight|hotel|note|ranking|poi):(\d+)$");
re!(
  USER,
  r"(?:^|[?&]clientAuth=)([0-9A-Fa-f]{32}(?:[0-9A-Fa-f]{32})?)\b"
);

/// A sight by whichever of its ids are known.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sight {
  pub sight: Option<String>,
  pub district: Option<String>,
  pub poi: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
  Sight(Sight),
  Hotel(String),
  Note(String),
  /// A travel note (游记) page.
  Travel(String),
  Ranking(String),
}

pub fn target(arg: &str) -> Result<Target> {
  let arg = arg.trim();
  if let Some(m) = PREFIXED.captures(arg) {
    let id = m[2].to_owned();
    return Ok(match &m[1] {
      "sight" => Target::Sight(Sight::id(id)),
      "poi" => Target::Sight(Sight {
        poi: Some(id),
        ..Sight::default()
      }),
      "hotel" => Target::Hotel(id),
      "note" => Target::Note(id),
      _ => Target::Ranking(id),
    });
  }
  link(arg).ok_or_else(|| {
    Error::input(format!(
      "not a Ctrip link or id: {arg} (give a sight, hotel, note or ranking link, \
       or sight:ID, hotel:ID, note:ID)"
    ))
  })
}

fn link(arg: &str) -> Option<Target> {
  if let Some(sight) = sight_link(arg) {
    return Some(Target::Sight(sight));
  }
  if let Some(m) = TRAVEL.captures(arg) {
    return Some(Target::Travel(format!(
      "{YOU}/travels/{}/{}.html",
      &m[1], &m[2]
    )));
  }
  if let Some(m) = RANKING.captures(arg) {
    return Some(Target::Ranking(m[1].to_owned()));
  }
  if let Some(m) = NOTE.captures(arg) {
    return Some(Target::Note(m[1].to_owned()));
  }
  HOTEL
    .captures(arg)
    .and_then(|m| {
      m.iter()
        .skip(1)
        .flatten()
        .next()
        .map(|g| g.as_str().to_owned())
    })
    .map(Target::Hotel)
}

fn sight_link(arg: &str) -> Option<Sight> {
  let poi = POI.captures(arg).map(|m| m[1].to_owned());
  let m = SIGHT_YOU.captures(arg).or_else(|| SIGHT_M.captures(arg))?;
  let known = |s: &str| Some(s.to_owned()).filter(|s| s != "0");
  Some(Sight {
    district: known(&m[1]),
    sight: known(&m[2]),
    poi,
  })
  .filter(|s| s.sight.is_some() || s.poi.is_some())
}

impl Sight {
  pub fn id(sight: String) -> Self {
    Self {
      sight: Some(sight),
      ..Self::default()
    }
  }
}

/// A sight given to `sight`: a link, `poi:ID`, or its sight id.
pub fn sight(arg: &str) -> Result<Sight> {
  let arg = arg.trim();
  if !arg.is_empty() && arg.bytes().all(|b| b.is_ascii_digit()) {
    return Ok(Sight::id(arg.to_owned()));
  }
  match target(arg)? {
    Target::Sight(sight) => Ok(sight),
    _ => Err(Error::input(format!("not a Ctrip sight: {arg}"))),
  }
}

/// A hotel given to `hotel` or `rooms`: a link, `hotel:ID` or its id.
pub fn hotel(arg: &str) -> Result<String> {
  let arg = arg.trim();
  if !arg.is_empty() && arg.bytes().all(|b| b.is_ascii_digit()) {
    return Ok(arg.to_owned());
  }
  match target(arg)? {
    Target::Hotel(id) => Ok(id),
    _ => Err(Error::input(format!("not a Ctrip hotel: {arg}"))),
  }
}

/// A ranking given to `ranking`: a link, `ranking:ID` or its id.
pub fn ranking(arg: &str) -> Result<String> {
  let arg = arg.trim();
  if !arg.is_empty() && arg.bytes().all(|b| b.is_ascii_digit()) {
    return Ok(arg.to_owned());
  }
  match target(arg)? {
    Target::Ranking(id) => Ok(id),
    _ => Err(Error::input(format!("not a Ctrip ranking: {arg}"))),
  }
}

/// A community user: the `clientAuth` (32 or 64 hex digits) or a link
/// carrying it.
pub fn user(arg: &str) -> Result<String> {
  USER
    .captures(arg.trim())
    .map(|m| m[1].to_ascii_uppercase())
    .ok_or_else(|| {
      Error::input(format!(
        "not a Ctrip user: {arg} (give the clientAuth or a community/user?clientAuth=… link)"
      ))
    })
}

/// A `YYYY-MM-DD` day.
pub fn day(text: &str) -> Result<Date> {
  text
    .trim()
    .parse()
    .map_err(|_| Error::input(format!("not a day: {text} (use YYYY-MM-DD)")))
}

/// Today in Beijing.
pub fn today() -> Date {
  let beijing = TimeZone::fixed(Offset::constant(8));
  jiff::Timestamp::now().to_zoned(beijing).date()
}

/// A numeric id as the services want it.
pub fn number(id: &str) -> Result<u64> {
  id.trim()
    .parse()
    .map_err(|_| Error::input(format!("not a Ctrip id: {id}")))
}

/// The mobile page of a sight, which names all three ids.
pub fn sight_url(sight: &Sight) -> String {
  let id = |v: &Option<String>| v.clone().unwrap_or_else(|| "0".into());
  let mut url = format!(
    "{M}/webapp/you/gspoi/sight/{}/{}.html",
    id(&sight.district),
    id(&sight.sight)
  );
  if let Some(poi) = &sight.poi {
    url.push_str(&format!("?poiId={poi}"));
  }
  url
}

/// The `you.ctrip.com` page of a sight; the site redirects a placeholder
/// slug and district to the right ones.
pub fn sight_page(sight: &str, district: Option<&str>) -> String {
  format!("{YOU}/sight/x{}/{sight}.html", district.unwrap_or("0"))
}

pub fn hotel_url(id: &str) -> String {
  format!("{HOTELS}/hotels/detail/?hotelid={id}")
}

pub fn note_url(id: &str) -> String {
  format!("{M}/webapp/you/community/detail?articleId={id}")
}

pub fn user_url(client_auth: &str) -> String {
  format!("{M}/webapp/you/community/user?clientAuth={client_auth}")
}

pub fn ranking_url(id: &str) -> String {
  format!("{M}/webapp/xranking/cranking/crankingCity/{id}.html")
}

/// The flight list of a day between two airport cities (codes).
pub fn flights_url(from: &str, to: &str, day: &str) -> String {
  format!(
    "{FLIGHTS}/online/list/oneway-{}-{}?depdate={day}",
    from.to_ascii_lowercase(),
    to.to_ascii_lowercase()
  )
}

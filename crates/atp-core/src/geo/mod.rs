//! What map platforms share: coordinates in their system, places, routes
//! and distances.
//!
//! Every coordinate carries its [`System`]: WGS-84 (GPS; Google outside
//! mainland China), GCJ-02 (Amap, Tencent; Google inside mainland China)
//! or BD-09 (Baidu). A platform tags what it parses with the system the
//! upstream used and converts what users give into the system its API
//! takes ([`Coord::to`]). `--coord` names the system of both: the shared
//! CLI hands it to the platform as [`Ctx::coord`](crate::Ctx) and converts
//! every coordinate of the result into it ([`localize`]).
//!
//! Places are posts with a [`Place`] (`Post.place`), so `search`, `read`,
//! `#N` and the tables work on them as on any post; a route plan is
//! [`Routes`] and a distance table a list of [`Distance`]s.

mod china;
mod coord;
mod datum;
mod image;
mod localize;
mod model;
mod polyline;

pub use self::coord::{Coord, System};
pub use self::image::{image_size, save_map};
pub use self::localize::localize;
pub use self::model::{Distance, Measure, Mode, Place, Route, Routes, Step, Waypoint};
pub use self::polyline::Polyline;

/// `850 m`, `12.3 km`.
pub fn fmt_distance(metres: u64) -> String {
  if metres < 1000 {
    format!("{metres} m")
  } else if metres < 100_000 {
    format!("{:.1} km", metres as f64 / 1000.0)
  } else {
    format!("{} km", (metres as f64 / 1000.0).round())
  }
}

/// `45 s`, `25 min`, `1 h 05 min`.
pub fn fmt_trip(secs: u64) -> String {
  match secs {
    0..60 => format!("{secs} s"),
    60..3600 => format!("{} min", (secs + 30) / 60),
    _ => {
      let minutes = (secs + 30) / 60;
      format!("{} h {:02} min", minutes / 60, minutes % 60)
    }
  }
}

//! Dianping-only commands: the city ids searches take, and the city at a
//! point (through Meituan's lookup, which knows Dianping's ids too).

use atp_core::{Collection, Data, Page, Result};
use atp_mtsec::geo;

use crate::api::Api;

/// Dianping's ids of the larger cities.
const CITIES: &[(&str, &str)] = &[
  ("1", "上海"),
  ("2", "北京"),
  ("3", "杭州"),
  ("4", "广州"),
  ("5", "南京"),
  ("6", "苏州"),
  ("7", "深圳"),
  ("8", "成都"),
  ("9", "重庆"),
  ("10", "天津"),
  ("16", "武汉"),
  ("17", "西安"),
  ("19", "大连"),
];

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Dianping's ids of the larger cities (set one in DIANPING_CITY for `search`)
  Cities {
    /// Only cities whose name contains this
    name: Option<String>,
  },
  /// The city at a point, with its Dianping id (any city)
  #[command(after_long_help = EX_LOCATE)]
  Locate {
    /// Latitude of the point
    #[arg(allow_negative_numbers = true)]
    lat: f64,
    /// Longitude of the point
    #[arg(allow_negative_numbers = true)]
    lng: f64,
  },
}

const EX_LOCATE: &str = "\
Examples:
  atp dp locate 30.2741 120.1551
  DIANPING_CITY=3 atp dp search 西湖醋鱼

More: atp guide dp locate";

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Cities { name } => Data::Collections(Page::last(cities(name.as_deref()))),
    Command::Locate { lat, lng } => Data::Value(geo::locate(&api.ctx, lat, lng).await?),
  })
}

fn cities(name: Option<&str>) -> Vec<Collection> {
  CITIES
    .iter()
    .filter(|(_, n)| name.is_none_or(|w| n.contains(w)))
    .map(|(id, n)| Collection {
      id: (*id).into(),
      kind: "city".into(),
      name: (*n).into(),
      ..Collection::default()
    })
    .collect()
}

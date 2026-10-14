//! Amap commands beyond the shared set.

use std::path::PathBuf;

use atp_core::cli::PageArgs;
use atp_core::{Data, Error, Result};

use crate::api::Api;
use crate::maps::{self, MapOptions};
use crate::routes::{self, Measure, Mode};
use crate::traffic::{self, Area};
use crate::{districts, geocoding, pois, weather};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Places around a point, nearest first
  Nearby(NearbyArgs),
  /// Address → coordinates
  Geocode {
    address: String,
    /// Narrow the search to a city (name, citycode or adcode)
    #[arg(long)]
    city: Option<String>,
  },
  /// Coordinates → address
  Regeo {
    /// LNG,LAT (GCJ-02)
    location: String,
    /// Also list places and roads nearby
    #[arg(long)]
    pois: bool,
  },
  /// Route between two places: distance, time, tolls and steps
  Route(RouteArgs),
  /// Distances from one or more places to a destination
  Distance {
    /// LNG,LAT or place names (up to 100)
    #[arg(required = true, num_args = 1..=100)]
    from: Vec<String>,
    #[arg(long)]
    to: String,
    #[arg(long, value_enum, default_value_t = Measure::Straight)]
    mode: Measure,
  },
  /// Suggestions for a keyword as you type it
  Tips {
    keyword: String,
    #[arg(long)]
    city: Option<String>,
    /// LNG,LAT to rank nearby suggestions first
    #[arg(long)]
    location: Option<String>,
  },
  /// Administrative districts and the levels below them
  District {
    /// Name or adcode (中国 for the provinces)
    name: String,
    /// Levels below to include (0–3)
    #[arg(long, default_value_t = 1)]
    depth: u8,
  },
  /// Where an IP address is (yours by default; mainland China only)
  Ip { ip: Option<String> },
  /// Weather now, or the forecast for the next days
  Weather {
    /// City or district name, or its adcode
    city: String,
    #[arg(long)]
    forecast: bool,
  },
  /// Live traffic on a road, in a rectangle or around a point
  Traffic(TrafficArgs),
  /// Convert GPS, Baidu or Mapbar coordinates to Amap's (GCJ-02)
  Convert {
    /// LNG,LAT; several separated by `|`
    coords: String,
    #[arg(long, default_value = "gps", value_parser = ["gps", "baidu", "mapbar"])]
    from: String,
  },
  /// Save a map image centred on a point
  Staticmap(MapArgs),
}

#[derive(Debug, clap::Args)]
pub struct NearbyArgs {
  /// LNG,LAT (GCJ-02)
  location: String,
  keyword: Option<String>,
  /// Search radius in metres (at most 50000)
  #[arg(long, default_value_t = 3000)]
  radius: u32,
  /// A category (food, hotel, parking ...) or an Amap type code
  #[arg(long)]
  filter: Option<String>,
  #[command(flatten)]
  page: PageArgs,
}

#[derive(Debug, clap::Args)]
pub struct RouteArgs {
  /// LNG,LAT or a place name
  from: String,
  /// LNG,LAT or a place name
  to: String,
  #[arg(long = "type", value_enum, default_value_t = Mode::Driving)]
  mode: Mode,
  /// City to look place names up in
  #[arg(long)]
  city: Option<String>,
  /// Driving: recommended, avoid-jam, highway, no-highway, least-fee, big-road, fastest, no-toll.
  /// Transit: recommended, cheapest, fewest-transfers, least-walk, comfortable, no-subway, subway, fastest
  #[arg(long)]
  strategy: Option<String>,
}

#[derive(Debug, clap::Args)]
pub struct TrafficArgs {
  /// Road name (needs --city)
  road: Option<String>,
  #[arg(long)]
  city: Option<String>,
  /// LNG,LAT;LNG,LAT: south-west and north-east corners
  #[arg(long, value_name = "RECT", conflicts_with_all = ["road", "around"])]
  rect: Option<String>,
  /// LNG,LAT: the centre of a circle
  #[arg(long, value_name = "LNG,LAT", conflicts_with = "road")]
  around: Option<String>,
  /// Circle radius in metres (at most 5000)
  #[arg(long, default_value_t = 1000)]
  radius: u32,
}

#[derive(Debug, clap::Args)]
pub struct MapArgs {
  /// LNG,LAT (GCJ-02)
  location: String,
  /// 1 (world) to 17 (street)
  #[arg(long, default_value_t = 15)]
  zoom: u8,
  /// WIDTH*HEIGHT in pixels, at most 1024*1024
  #[arg(long, default_value = "600*400")]
  size: String,
  /// 2 for high-DPI images
  #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=2))]
  scale: u8,
  /// Draw live traffic
  #[arg(long)]
  traffic: bool,
  #[arg(
    short = 'o',
    long,
    value_name = "FILE",
    default_value = "staticmap.png"
  )]
  output: PathBuf,
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Nearby(args) => nearby(api, args).await?,
    Command::Geocode { address, city } => {
      Data::Value(geocoding::geocode(api, &address, city.as_deref()).await?)
    }
    Command::Regeo { location, pois } => Data::Value(geocoding::regeo(api, &location, pois).await?),
    Command::Route(args) => route(api, args).await?,
    Command::Distance { from, to, mode } => {
      Data::Value(routes::distance(api, &from, &to, mode).await?)
    }
    Command::Tips {
      keyword,
      city,
      location,
    } => Data::Posts(pois::tips(api, &keyword, city.as_deref(), location.as_deref()).await?),
    Command::District { name, depth } => {
      Data::Collections(districts::districts(api, &name, depth).await?)
    }
    Command::Ip { ip } => Data::Value(geocoding::ip(api, ip.as_deref()).await?),
    Command::Weather { city, forecast } => {
      Data::Value(weather::weather(api, &city, forecast).await?)
    }
    Command::Traffic(args) => Data::Value(traffic::status(api, area(&args)?).await?),
    Command::Convert { coords, from } => {
      Data::Value(geocoding::convert(api, &coords, &from).await?)
    }
    Command::Staticmap(args) => staticmap(api, args).await?,
  })
}

async fn nearby(api: &Api, args: NearbyArgs) -> Result<Data> {
  let keyword = args.keyword.as_deref();
  let filter = args.filter.as_deref();
  let page = args
    .page
    .collect(async |req| {
      pois::nearby(api, &args.location, keyword, filter, args.radius, &req).await
    })
    .await?;
  Ok(Data::Posts(page))
}

async fn route(api: &Api, args: RouteArgs) -> Result<Data> {
  let city = args.city.as_deref();
  let strategy = args.strategy.as_deref();
  let route = routes::route(api, &args.from, &args.to, args.mode, city, strategy).await?;
  Ok(Data::Value(route))
}

/// The area `traffic` looks at, from its arguments.
fn area(args: &TrafficArgs) -> Result<Area<'_>> {
  if let Some(rect) = &args.rect {
    return Ok(Area::Rect(rect));
  }
  if let Some(location) = &args.around {
    let radius = args.radius;
    return Ok(Area::Around { location, radius });
  }
  let (Some(name), Some(city)) = (&args.road, &args.city) else {
    return Err(Error::input(
      "give ROAD --city CITY, --rect LNG,LAT;LNG,LAT or --around LNG,LAT",
    ));
  };
  Ok(Area::Road { name, city })
}

async fn staticmap(api: &Api, args: MapArgs) -> Result<Data> {
  let options = MapOptions {
    zoom: args.zoom,
    size: &args.size,
    scale: args.scale,
    traffic: args.traffic,
  };
  maps::staticmap(api, &args.location, &options, &args.output).await
}

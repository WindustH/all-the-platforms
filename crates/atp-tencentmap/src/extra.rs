//! Tencent Map commands beyond the shared set.

use std::path::PathBuf;

use atp_core::cli::PageArgs;
use atp_core::geo::{Measure, Mode};
use atp_core::{Data, Result};

use crate::api::Api;
use crate::maps::{self, MapOptions};
use crate::{districts, geocoding, places, routes, weather};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Places around a point, nearest first
  #[command(after_long_help = EX_NEARBY)]
  Nearby(NearbyArgs),
  /// Address → coordinates
  Geocode {
    /// Address to look up
    address: String,
    /// City the address is in (put in front of it)
    #[arg(long)]
    city: Option<String>,
  },
  /// Coordinates → address
  Regeo {
    /// LNG,LAT
    location: String,
    /// Also list places nearby
    #[arg(long)]
    pois: bool,
  },
  /// Routes between two places: distance, time, tolls and steps
  #[command(after_long_help = EX_ROUTE)]
  Route(RouteArgs),
  /// Distances from places to destinations
  #[command(after_long_help = EX_DISTANCE)]
  Distance {
    /// LNG,LAT or place names (origins × destinations: at most 625)
    #[arg(required = true, num_args = 1..)]
    from: Vec<String>,
    /// Destination: LNG,LAT or a place name; repeat for several
    #[arg(long, required = true)]
    to: Vec<String>,
    /// Straight-line, driving, walking, cycling or ebike distance
    #[arg(long, value_enum, default_value_t = Measure::Straight)]
    mode: Measure,
  },
  /// Suggestions for a keyword as you type it
  Tips {
    /// The keyword typed so far
    keyword: String,
    /// Keep suggestions inside a city (name or adcode)
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
    /// Levels below to include (0–2)
    #[arg(long, default_value_t = 1)]
    depth: u8,
  },
  /// Where an IP address is (yours by default)
  Ip {
    /// IP address to locate (IPv4 or IPv6)
    ip: Option<String>,
  },
  /// Weather now, or the forecast for the next week
  Weather {
    /// City or district name, its adcode, or LNG,LAT
    city: String,
    /// The forecast for the next seven days instead of the weather now
    #[arg(long)]
    forecast: bool,
  },
  /// Save a map image centred on a point
  Staticmap(MapArgs),
}

const EX_NEARBY: &str = "\
Examples:
  atp qqmap nearby 116.307,39.984 咖啡 --radius 800
  atp qqmap nearby 116.307,39.984 --filter food

More: atp guide qqmap nearby";

const EX_ROUTE: &str = "\
Examples:
  atp qqmap route 116.307,39.984 116.397,39.909
  atp qqmap route 中关村 北京南站 --type transit --strategy least-walk --city 北京

More: atp guide qqmap route";

const EX_DISTANCE: &str = "\
Examples:
  atp qqmap distance 116.307,39.984 116.434,39.908 --to 116.397,39.909
  atp qqmap distance 中关村 --to 国贸 --to 北京南站 --mode driving

More: atp guide qqmap distance";

#[derive(Debug, clap::Args)]
pub struct NearbyArgs {
  /// LNG,LAT
  location: String,
  /// Keyword the places must match; without one, Tencent recommends places
  keyword: Option<String>,
  /// Search radius in metres (at most 1000)
  #[arg(long, default_value_t = 1000)]
  radius: u32,
  /// A category (food, hotel, parking ...) or a Tencent category name
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
  /// How to travel
  #[arg(long = "type", value_enum, default_value_t = Mode::Driving)]
  mode: Mode,
  /// City to look place names up in
  #[arg(long)]
  city: Option<String>,
  /// Driving: recommended, avoid-jam, least-fee, highway, no-highway, big-road.
  /// Transit: recommended, fastest, fewest-transfers, least-walk, no-subway, subway
  #[arg(long)]
  strategy: Option<String>,
}

#[derive(Debug, clap::Args)]
pub struct MapArgs {
  /// LNG,LAT
  location: String,
  /// 4 (country) to 18 (street)
  #[arg(long, default_value_t = 15)]
  zoom: u8,
  /// WIDTH*HEIGHT in pixels, at most 1680*1200
  #[arg(long, default_value = "600*400")]
  size: String,
  /// 2 for high-DPI images
  #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=2))]
  scale: u8,
  /// File to save the image to
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
      Data::Posts(geocoding::geocode(api, &address, city.as_deref()).await?)
    }
    Command::Regeo { location, pois } => geocoding::regeo(api, &location, pois).await?,
    Command::Route(args) => route(api, args).await?,
    Command::Distance { from, to, mode } => {
      Data::Distances(routes::distance(api, &from, &to, mode).await?)
    }
    Command::Tips {
      keyword,
      city,
      location,
    } => Data::Posts(places::tips(api, &keyword, city.as_deref(), location.as_deref()).await?),
    Command::District { name, depth } => {
      Data::Collections(districts::districts(api, &name, depth).await?)
    }
    Command::Ip { ip } => Data::Value(geocoding::ip(api, ip.as_deref()).await?),
    Command::Weather { city, forecast } => {
      Data::Value(weather::weather(api, &city, forecast).await?)
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
      places::nearby(api, &args.location, keyword, filter, args.radius, &req).await
    })
    .await?;
  Ok(Data::Posts(page))
}

async fn route(api: &Api, args: RouteArgs) -> Result<Data> {
  let city = args.city.as_deref();
  let strategy = args.strategy.as_deref();
  let route = routes::route(api, &args.from, &args.to, args.mode, city, strategy).await?;
  Ok(Data::Routes(Box::new(route)))
}

async fn staticmap(api: &Api, args: MapArgs) -> Result<Data> {
  let options = MapOptions {
    zoom: args.zoom,
    size: &args.size,
    scale: args.scale,
  };
  maps::staticmap(api, &args.location, &options, &args.output).await
}

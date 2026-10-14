//! Baidu Map commands beyond the shared set.

use std::path::PathBuf;

use atp_core::cli::PageArgs;
use atp_core::geo::{Measure, Mode};
use atp_core::{Data, Error, Result};

use crate::api::Api;
use crate::maps::{self, MapOptions};
use crate::traffic::{self, Area};
use crate::{geocoding, places, routes, weather};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Places around a point
  #[command(after_long_help = EX_NEARBY)]
  Nearby(NearbyArgs),
  /// Address → coordinates
  Geocode {
    /// Address to look up
    address: String,
    /// Narrow the search to a city
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
    /// LNG,LAT or place names (origins × destinations: at most 100
    /// driving, 50 otherwise)
    #[arg(required = true, num_args = 1..)]
    from: Vec<String>,
    /// Destination: LNG,LAT or a place name; repeat for several
    #[arg(long, required = true)]
    to: Vec<String>,
    /// Straight-line, driving, walking, cycling or ebike distance
    #[arg(long, value_enum, default_value_t = Measure::Straight)]
    mode: Measure,
  },
  /// Suggestions for a keyword as you type it (at most 10)
  Tips {
    /// The keyword typed so far
    keyword: String,
    /// Keep suggestions inside a city
    #[arg(long)]
    city: Option<String>,
    /// LNG,LAT to rank nearby suggestions first
    #[arg(long)]
    location: Option<String>,
  },
  /// Where an IP address is (yours by default)
  Ip {
    /// IP address to locate
    ip: Option<String>,
  },
  /// Weather now, or the forecast for the next days
  Weather {
    /// District adcode, LNG,LAT or a place name
    city: String,
    /// The forecast for the next days instead of the weather now
    #[arg(long)]
    forecast: bool,
  },
  /// Live traffic on a road, in a rectangle or around a point
  #[command(after_long_help = EX_TRAFFIC)]
  Traffic(TrafficArgs),
  /// Save a map image centred on a point
  Staticmap(MapArgs),
}

const EX_NEARBY: &str = "\
Examples:
  atp bmap nearby 116.410,39.922 咖啡 --radius 1000
  atp bmap nearby 116.410,39.922 --filter food

More: atp guide bmap nearby";

const EX_ROUTE: &str = "\
Examples:
  atp bmap route 116.346,40.017 116.459,39.942
  atp bmap route 西直门 北京南站 --type transit --strategy least-walk --city 北京

More: atp guide bmap route";

const EX_DISTANCE: &str = "\
Examples:
  atp bmap distance 116.346,40.017 116.459,39.942 --to 116.404,39.915
  atp bmap distance 西直门 国贸 --to 北京南站 --mode driving

More: atp guide bmap distance";

const EX_TRAFFIC: &str = "\
Examples:
  atp bmap traffic 长安街 --city 北京
  atp bmap traffic --rect '116.404,39.915;116.414,39.925'
  atp bmap traffic --around 116.404,39.915 --radius 800

More: atp guide bmap traffic";

#[derive(Debug, clap::Args)]
pub struct NearbyArgs {
  /// LNG,LAT
  location: String,
  /// Keyword the places must match (or give --filter)
  keyword: Option<String>,
  /// Search radius in metres
  #[arg(long, default_value_t = 3000)]
  radius: u32,
  /// A category (food, hotel, parking ...) or a Baidu category name
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
  /// Driving: recommended, shortest, no-highway, highway, avoid-jam, least-fee, fastest.
  /// Transit: recommended, fewest-transfers, least-walk, no-subway, fastest, subway
  #[arg(long)]
  strategy: Option<String>,
}

#[derive(Debug, clap::Args)]
pub struct TrafficArgs {
  /// Road name (needs --city)
  road: Option<String>,
  /// City of the road
  #[arg(long)]
  city: Option<String>,
  /// LNG,LAT;LNG,LAT: south-west and north-east corners (at most 2 km apart)
  #[arg(long, value_name = "RECT", conflicts_with_all = ["road", "around"])]
  rect: Option<String>,
  /// LNG,LAT: the centre of a circle
  #[arg(long, value_name = "LNG,LAT", conflicts_with = "road")]
  around: Option<String>,
  /// Circle radius in metres (at most 1000)
  #[arg(long, default_value_t = 1000)]
  radius: u32,
}

#[derive(Debug, clap::Args)]
pub struct MapArgs {
  /// LNG,LAT
  location: String,
  /// 3 (country) to 18 (street)
  #[arg(long, default_value_t = 15)]
  zoom: u8,
  /// WIDTH*HEIGHT in pixels, at most 1024*1024 (512*512 with --scale 2)
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
    Command::Ip { ip } => Data::Value(geocoding::ip(api, ip.as_deref()).await?),
    Command::Weather { city, forecast } => {
      Data::Value(weather::weather(api, &city, forecast).await?)
    }
    Command::Traffic(args) => Data::Value(traffic::status(api, area(&args)?).await?),
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
  };
  maps::staticmap(api, &args.location, &options, &args.output).await
}

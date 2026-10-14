//! Google Maps commands beyond the shared set.

use std::path::PathBuf;

use atp_core::cli::PageArgs;
use atp_core::geo::{Measure, Mode};
use atp_core::text::parse_when;
use atp_core::{Data, Result};

use crate::api::Api;
use crate::maps::{self, MapOptions};
use crate::{elevations, geocoding, places, routes, timezones};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Places around a point: nearest first, or matching a keyword
  #[command(after_long_help = EX_NEARBY)]
  Nearby(NearbyArgs),
  /// Address → coordinates
  Geocode {
    /// Address to look up
    address: String,
    /// City the address is in (biases the results)
    #[arg(long)]
    city: Option<String>,
  },
  /// Coordinates → address
  Regeo {
    /// LNG,LAT
    #[arg(allow_hyphen_values = true)]
    location: String,
    /// Also list places within 100 m (a Nearby Search, billed at Pro)
    #[arg(long)]
    pois: bool,
  },
  /// Routes between two places: distance, time and steps
  #[command(after_long_help = EX_ROUTE)]
  Route(RouteArgs),
  /// Distances from places to destinations
  #[command(after_long_help = EX_DISTANCE)]
  Distance {
    /// LNG,LAT or addresses (origins × destinations: at most 625)
    #[arg(required = true, num_args = 1..)]
    from: Vec<String>,
    /// Destination: LNG,LAT or an address; repeat for several
    #[arg(long, required = true, allow_hyphen_values = true)]
    to: Vec<String>,
    /// Straight-line, driving, walking, cycling or ebike distance
    #[arg(long, value_enum, default_value_t = Measure::Straight)]
    mode: Measure,
  },
  /// Suggestions for what you are typing: places and queries
  Tips {
    /// The text typed so far
    keyword: String,
    /// City to look in (added to the text)
    #[arg(long)]
    city: Option<String>,
    /// LNG,LAT to rank nearby suggestions first and measure distances
    #[arg(long, allow_hyphen_values = true)]
    location: Option<String>,
  },
  /// Time zone and local time at a point
  Timezone {
    /// LNG,LAT
    #[arg(allow_hyphen_values = true)]
    location: String,
    /// The moment to ask about (daylight saving): 2026-09-01, an RFC 3339
    /// time, or 7d for a week ago; now by default
    #[arg(long, value_parser = parse_when, value_name = "WHEN")]
    at: Option<jiff::Timestamp>,
  },
  /// Elevation above sea level at points
  Elevation {
    /// LNG,LAT points (at most 512)
    #[arg(required = true, num_args = 1..=512)]
    points: Vec<String>,
  },
  /// Save a map image centred on a point
  Staticmap(MapArgs),
}

const EX_NEARBY: &str = "\
Examples:
  atp gmap nearby -122.084,37.422 --filter cafe --radius 800
  atp gmap nearby 2.2945,48.8584 bakery --radius 1500

More: atp guide gmap nearby";

const EX_ROUTE: &str = "\
Examples:
  atp gmap route -122.084,37.422 -122.419,37.775
  atp gmap route 'Times Square' 'JFK Airport' --type transit --strategy least-walk --city 'New York'

More: atp guide gmap route";

const EX_DISTANCE: &str = "\
Examples:
  atp gmap distance 'Berlin Hbf' 'Potsdam Hbf' --to 'BER Airport'
  atp gmap distance 2.2945,48.8584 --to 2.3376,48.8606 --to 2.3499,48.8530 --mode walking

More: atp guide gmap distance";

#[derive(Debug, clap::Args)]
pub struct NearbyArgs {
  /// LNG,LAT
  #[arg(allow_hyphen_values = true)]
  location: String,
  /// Keyword the places must match (searches by relevance near the point)
  keyword: Option<String>,
  /// Search radius in metres (at most 50000)
  #[arg(long, default_value_t = 3000)]
  radius: u32,
  /// A category (food, cafe, hotel, parking ...) or a Google place type
  #[arg(long)]
  filter: Option<String>,
  #[command(flatten)]
  page: PageArgs,
}

#[derive(Debug, clap::Args)]
pub struct RouteArgs {
  /// LNG,LAT or an address
  #[arg(allow_hyphen_values = true)]
  from: String,
  /// LNG,LAT or an address
  #[arg(allow_hyphen_values = true)]
  to: String,
  /// How to travel
  #[arg(long = "type", value_enum, default_value_t = Mode::Driving)]
  mode: Mode,
  /// City added to addresses
  #[arg(long)]
  city: Option<String>,
  /// Driving and ebike: recommended, avoid-jam, fastest, no-toll, least-fee, no-highway, no-ferry.
  /// Transit: recommended, least-walk, fewest-transfers, subway, no-subway, bus, rail
  #[arg(long)]
  strategy: Option<String>,
}

#[derive(Debug, clap::Args)]
pub struct MapArgs {
  /// LNG,LAT
  #[arg(allow_hyphen_values = true)]
  location: String,
  /// 0 (world) to 21 (buildings)
  #[arg(long, default_value_t = 15)]
  zoom: u8,
  /// WIDTH*HEIGHT in pixels, at most 640*640
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
    Command::Timezone { location, at } => {
      let at = at.map(|t| t.as_second());
      Data::Value(timezones::timezone(api, &location, at).await?)
    }
    Command::Elevation { points } => Data::Value(elevations::elevation(api, &points).await?),
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

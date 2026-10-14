//! Ctrip-only commands: places and cities, sights, rankings, hotels and
//! rooms, trains and transfers, flight fares.

use atp_core::cli::PageArgs;
use atp_core::{Data, Result};

use crate::api::Api;
use crate::fares::{self, Route};
use crate::hotels::{self, Stay};
use crate::sights::{self, Filter};
use crate::{cities, rankings, refs, search, trains};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Destinations, stations and hotels suggested for a keyword
  #[command(after_long_help = EX_SUGGEST)]
  Suggest {
    /// Words to look up (a city, sight, station or hotel name)
    keyword: String,
    /// Places of the hotel search (hotels, business areas) instead of destinations
    #[arg(long)]
    hotels: bool,
  },
  /// Places of a name, with the district id (sights, rankings) and the city id (hotels)
  Cities {
    /// City or place name (杭州)
    name: String,
  },
  /// A city's sights: score, reviews, heat, level and ticket price
  #[command(after_long_help = EX_SIGHTS)]
  Sights(SightsArgs),
  /// One sight in full: scores, hours, tickets, phones, coordinates and photos
  #[command(after_long_help = EX_SIGHT)]
  Sight {
    /// Sight id, link, poi:ID or #N
    sight: String,
  },
  /// A city's rankings (必打卡, 夜游, 亲子 …)
  Rankings {
    /// City name or district id
    city: String,
  },
  /// The entries of a ranking
  Ranking {
    /// Ranking id, link or #N of `rankings`
    ranking: String,
  },
  /// A city's hotels with scores and sub-scores (the list's first screen)
  Hotels(HotelsArgs),
  /// One hotel: scores, sub-scores, facilities, policies and photos
  Hotel {
    /// Hotel id, link or #N
    hotel: String,
  },
  /// Room plans and prices for a stay (needs a login)
  #[command(after_long_help = EX_ROOMS)]
  Rooms(RoomsArgs),
  /// The day's trains between two stations or cities: seats, fares, tickets left
  #[command(after_long_help = EX_TRAINS)]
  Trains(TrainArgs),
  /// The day's plans with one change (train and train, or a flight and a train)
  Transfers(TransferArgs),
  /// The lowest one-way flight fare of each day between two cities
  #[command(after_long_help = EX_FARES)]
  Fares(FareArgs),
}

const EX_SUGGEST: &str = "\
Examples:
  atp ctrip suggest 西湖
  atp ctrip suggest 外滩 --hotels

More: atp guide ctrip suggest";

const EX_SIGHTS: &str = "\
Examples:
  atp ctrip sights 杭州 --sort rating
  atp ctrip sights 14 --5a --free

More: atp guide ctrip sights";

const EX_SIGHT: &str = "\
Examples:
  atp ctrip sight 49894
  atp ctrip sight poi:82922
  atp ctrip sight '#3'

More: atp guide ctrip sight";

const EX_ROOMS: &str = "\
Examples:
  atp ctrip rooms 445394
  atp ctrip rooms hotel:445394 --checkin 2026-11-01 --checkout 2026-11-03 --adults 1

More: atp guide ctrip rooms";

const EX_TRAINS: &str = "\
Examples:
  atp ctrip trains 上海 北京
  atp ctrip trains 上海虹桥 杭州东 --date 2026-11-01

More: atp guide ctrip trains";

const EX_FARES: &str = "\
Examples:
  atp ctrip fares 上海 成都
  atp ctrip fares SHA CTU --date 2026-12-01 --direct

More: atp guide ctrip fares";

#[derive(Debug, clap::Args)]
pub struct SightsArgs {
  /// City name or district id
  city: String,
  /// Order of the list
  #[arg(long, default_value = "default", value_parser = ["default", "rating", "hot", "sales"])]
  sort: String,
  /// Only 5A sights
  #[arg(long = "5a")]
  level_5a: bool,
  /// Only free sights
  #[arg(long)]
  free: bool,
  #[command(flatten)]
  page: PageArgs,
}

#[derive(Debug, clap::Args)]
pub struct HotelsArgs {
  /// City name or hotel city id
  city: String,
  #[command(flatten)]
  stay: StayArgs,
}

#[derive(Debug, clap::Args)]
pub struct RoomsArgs {
  /// Hotel id, link or #N
  hotel: String,
  #[command(flatten)]
  stay: StayArgs,
  /// Adults staying
  #[arg(long, default_value_t = 2)]
  adults: u8,
}

#[derive(Debug, clap::Args)]
pub struct StayArgs {
  /// YYYY-MM-DD (today by default)
  #[arg(long)]
  checkin: Option<String>,
  /// YYYY-MM-DD (the day after check-in by default)
  #[arg(long)]
  checkout: Option<String>,
}

#[derive(Debug, clap::Args)]
pub struct TrainArgs {
  /// Station or city (上海, 上海虹桥)
  from: String,
  /// Station or city
  to: String,
  /// YYYY-MM-DD (today by default)
  #[arg(long)]
  date: Option<String>,
}

#[derive(Debug, clap::Args)]
pub struct TransferArgs {
  #[command(flatten)]
  trip: TrainArgs,
  #[command(flatten)]
  page: PageArgs,
}

#[derive(Debug, clap::Args)]
pub struct FareArgs {
  /// City name (上海) or airport-city code (SHA)
  from: String,
  /// City name or airport-city code
  to: String,
  /// First day, YYYY-MM-DD (today by default)
  #[arg(long)]
  date: Option<String>,
  /// Only nonstop flights
  #[arg(long)]
  direct: bool,
  #[command(flatten)]
  page: PageArgs,
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Suggest { keyword, hotels } => {
      Data::Posts(search::suggest(api, &keyword, hotels).await?)
    }
    Command::Cities { name } => Data::Collections(cities::cities(api, &name).await?),
    Command::Sights(args) => sights(api, args).await?,
    Command::Sight { sight } => {
      let sight = refs::sight(&api.ctx.post_ref(&sight)?)?;
      Data::Post(Box::new(sights::detail(api, &sight).await?))
    }
    Command::Rankings { city } => {
      let district = cities::district(api, Some(&city)).await?;
      Data::Collections(rankings::rankings(api, &district).await?)
    }
    Command::Ranking { ranking } => {
      let id = refs::ranking(&api.ctx.collection_ref(&ranking)?)?;
      Data::Posts(rankings::ranking(api, &id).await?)
    }
    Command::Hotels(args) => {
      let city = cities::city(api, &args.city).await?;
      Data::Posts(hotels::list(api, &city, &stay(&args.stay)?).await?)
    }
    Command::Hotel { hotel } => {
      let id = refs::hotel(&api.ctx.post_ref(&hotel)?)?;
      Data::Post(Box::new(hotels::read(api, &id).await?))
    }
    Command::Rooms(args) => rooms(api, args).await?,
    Command::Trains(args) => {
      let date = day_or_today(args.date.as_deref())?;
      Data::Posts(trains::trains(api, &args.from, &args.to, date).await?)
    }
    Command::Transfers(args) => transfers(api, args).await?,
    Command::Fares(args) => fares(api, args).await?,
  })
}

async fn sights(api: &Api, args: SightsArgs) -> Result<Data> {
  let district = cities::district(api, Some(&args.city)).await?;
  let filter = Filter {
    sort: sights::SORTS
      .iter()
      .find(|(name, _)| *name == args.sort)
      .map_or(1, |(_, id)| *id),
    city_only: true,
    level_5a: args.level_5a,
    free: args.free,
  };
  let page = args
    .page
    .collect(async |req| sights::list(api, &district, &filter, &req).await)
    .await?;
  Ok(Data::Posts(page))
}

async fn rooms(api: &Api, args: RoomsArgs) -> Result<Data> {
  let id = refs::hotel(&api.ctx.post_ref(&args.hotel)?)?;
  let stay = stay(&args.stay)?;
  Ok(Data::Posts(
    hotels::rooms(api, &id, &stay, args.adults).await?,
  ))
}

fn stay(args: &StayArgs) -> Result<Stay> {
  Stay::new(args.checkin.as_deref(), args.checkout.as_deref())
}

async fn transfers(api: &Api, args: TransferArgs) -> Result<Data> {
  let trip = &args.trip;
  let date = day_or_today(trip.date.as_deref())?;
  let page = args
    .page
    .collect(async |req| trains::transfers(api, &trip.from, &trip.to, date, &req).await)
    .await?;
  Ok(Data::Posts(page))
}

async fn fares(api: &Api, args: FareArgs) -> Result<Data> {
  let start = day_or_today(args.date.as_deref())?;
  let route = Route {
    from: &args.from,
    to: &args.to,
    direct: args.direct,
  };
  let page = args
    .page
    .collect(async |req| fares::fares(api, &route, start, &req).await)
    .await?;
  Ok(Data::Posts(page))
}

fn day_or_today(day: Option<&str>) -> Result<jiff::civil::Date> {
  day.map_or_else(|| Ok(refs::today()), refs::day)
}

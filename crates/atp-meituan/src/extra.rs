//! Meituan-only commands: the place at a point or an IP address, a city's
//! districts and business areas (both open to visitors), and deals with
//! every filter the 美团联盟 API takes.

use atp_core::cli::PageArgs;
use atp_core::{Data, Error, Page, PageReq, Result, paging};
use atp_mtsec::geo;

use crate::api::Api;
use crate::deals::{self, Filter, Rank, Sort};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// The city (Meituan's and Dianping's ids), district and business area at a point, or where an IP address is
  #[command(after_long_help = EX_LOCATE)]
  Locate {
    /// Latitude of the point
    #[arg(allow_negative_numbers = true, required_unless_present = "ip")]
    lat: Option<f64>,
    /// Longitude of the point
    #[arg(allow_negative_numbers = true, required_unless_present = "ip")]
    lng: Option<f64>,
    /// Locate an IP address instead (yours when none is given; mainland China only)
    #[arg(long, num_args = 0..=1, default_missing_value = "", conflicts_with_all = ["lat", "lng"])]
    ip: Option<String>,
  },
  /// A city's districts, or one district's business areas (ids for `deals --area`)
  #[command(after_long_help = EX_AREAS)]
  Areas {
    /// Meituan city id (from `locate`)
    city: String,
    /// District id or name
    #[arg(long)]
    district: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Deals from the 美团联盟 API: a ranking, a list or a search, in a city or near a point
  #[command(after_long_help = EX_DEALS)]
  Deals(DealsArgs),
}

const EX_LOCATE: &str = "\
Examples:
  atp mt locate 31.2304 121.4737
  atp mt locate --ip
  atp mt locate --ip 114.114.114.114

More: atp guide mt locate";

const EX_AREAS: &str = "\
Examples:
  atp mt areas 10
  atp mt areas 10 --district 浦东新区

More: atp guide mt areas";

const EX_DEALS: &str = "\
Examples:
  atp mt deals 火锅 --city 10
  atp mt deals --rank picks
  atp mt deals --store --near 31.2304,121.4737 --sort sales

More: atp guide mt deals";

#[derive(Debug, clap::Args)]
pub struct DealsArgs {
  /// Words to search for (searches cover every category)
  keyword: Option<String>,
  /// A ranking instead of a list (picks and trending are 到家, realtime 到店)
  #[arg(long, value_enum)]
  rank: Option<Rank>,
  /// 到店 deals (dining, leisure, hotels, tickets) instead of 到家 (delivery)
  #[arg(long)]
  store: bool,
  /// Meituan city id (from `locate`)
  #[arg(long)]
  city: Option<String>,
  /// Business area id (from `areas`)
  #[arg(long)]
  area: Option<String>,
  /// LAT,LNG to list around (Meituan answers for Beijing without one)
  #[arg(long, value_parser = point, allow_hyphen_values = true)]
  near: Option<(f64, f64)>,
  /// Order of the list (lists without words ignore commission; searches take price only)
  #[arg(long, value_enum)]
  sort: Option<Sort>,
  #[command(flatten)]
  page: PageArgs,
}

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Locate { lat, lng, ip } => Data::Value(locate(api, lat, lng, ip).await?),
    Command::Areas {
      city,
      district,
      page,
    } => {
      let areas = geo::areas(&api.ctx, &city, district.as_deref()).await?;
      let req = PageReq {
        cursor: page.cursor,
        size: page.limit,
      };
      Data::Collections(paging::slice(areas, &req))
    }
    Command::Deals(args) => Data::Posts(deals(api, args).await?),
  })
}

async fn locate(
  api: &Api,
  lat: Option<f64>,
  lng: Option<f64>,
  ip: Option<String>,
) -> Result<atp_core::Value> {
  match (ip, lat, lng) {
    (Some(ip), _, _) => geo::ip(&api.ctx, Some(ip.as_str()).filter(|s| !s.is_empty())).await,
    (None, Some(lat), Some(lng)) => geo::locate(&api.ctx, lat, lng).await,
    _ => Err(Error::input("give LAT LNG, or --ip")),
  }
}

async fn deals(api: &Api, args: DealsArgs) -> Result<Page<atp_core::Post>> {
  let filter = Filter {
    keyword: args.keyword,
    rank: args.rank,
    store: args.store,
    city: args.city,
    area: args.area,
    near: args.near,
    sort: args.sort,
  };
  args
    .page
    .collect(async |r| deals::list(api, &filter, &r).await)
    .await
}

/// `LAT,LNG`.
fn point(text: &str) -> std::result::Result<(f64, f64), String> {
  let (lat, lng) = text
    .split_once(',')
    .ok_or_else(|| "expected LAT,LNG".to_owned())?;
  let parse = |s: &str| s.trim().parse::<f64>().map_err(|e| format!("{s}: {e}"));
  Ok((parse(lat)?, parse(lng)?))
}

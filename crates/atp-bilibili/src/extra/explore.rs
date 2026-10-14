//! Discovery and reading: trends, weekly picks, topics, bangumi, live rooms
//! and a user's collections / articles.

use atp_core::cli::PageArgs;
use atp_core::{Data, Result};

use crate::api::Api;
use crate::{articles, bangumi, discover, lists, rooms, videos};

#[derive(Debug, clap::Subcommand)]
pub enum ExploreCommand {
  #[command(flatten)]
  Discovery(DiscoveryCommand),
  #[command(flatten)]
  Pgc(PgcCommand),
  #[command(flatten)]
  Rooms(RoomCommand),
  #[command(flatten)]
  Spaces(SpaceCommand),
}

pub async fn run(api: &Api, command: ExploreCommand) -> Result<Data> {
  match command {
    ExploreCommand::Discovery(c) => run_discovery(api, c).await,
    ExploreCommand::Pgc(c) => run_pgc(api, c).await,
    ExploreCommand::Rooms(c) => run_rooms(api, c).await,
    ExploreCommand::Spaces(c) => run_spaces(api, c).await,
  }
}

/// Trending searches, weekly and all-time picks, topic feeds.
#[derive(Debug, clap::Subcommand)]
pub enum DiscoveryCommand {
  /// Trending searches (热搜)
  Trending {
    /// How many words
    #[arg(short = 'n', long, default_value_t = 20)]
    limit: usize,
  },
  /// One issue of the weekly picks (每周必看); the latest by default
  Weekly {
    /// Issue number
    number: Option<u64>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// The all-time picks (入站必刷)
  Precious {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Dynamics under a topic (话题): its id or link
  #[command(after_long_help = EX_TOPIC)]
  Topic {
    /// Topic: id, a link with `topic_id=`, or #N after `search -t topic`
    topic: String,
    /// Order: most popular or newest first; Bilibili's default when omitted
    #[arg(long, value_parser = ["hot", "new"])]
    sort: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
}

async fn run_discovery(api: &Api, command: DiscoveryCommand) -> Result<Data> {
  use DiscoveryCommand as E;
  Ok(match command {
    E::Trending { limit } => Data::Collections(discover::trending(api, limit).await?),
    E::Weekly { number, page } => Data::Posts(
      page
        .collect_dated(async |_| discover::weekly(api, number).await)
        .await?,
    ),
    E::Precious { page } => Data::Posts(
      page
        .collect_dated(async |_| discover::precious(api).await)
        .await?,
    ),
    E::Topic { topic, sort, page } => {
      let topic = api.ctx.collection_ref(&topic)?;
      Data::Posts(
        page
          .collect_dated(async |r| discover::topic(api, &topic, sort.as_deref(), &r).await)
          .await?,
      )
    }
  })
}

/// Bangumi, films and documentaries: seasons, following, the timeline.
#[derive(Debug, clap::Subcommand)]
pub enum PgcCommand {
  /// Episodes of a bangumi / film / documentary (ss…, ep…, md… or link)
  #[command(after_long_help = EX_BANGUMI)]
  Bangumi {
    /// Season: ss…, ep…, md…, a /bangumi/ link or #N
    season: String,
    /// The season itself: rating, status, counters
    #[arg(long)]
    info: bool,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Follow (追番 / 追剧) a bangumi or film, or stop with --undo
  BangumiFollow {
    /// Season: ss…, ep…, md…, a /bangumi/ link or #N
    season: String,
    /// Stop following
    #[arg(long)]
    undo: bool,
  },
  /// Bangumi a user follows (yours when USER is omitted); --drama for films and series
  MyBangumi {
    /// User: mid, `uid:MID`, space link, name or #N; yours when omitted
    user: Option<String>,
    /// Films and series (追剧) instead of bangumi (追番)
    #[arg(long)]
    drama: bool,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Episodes airing around today (新番时间表)
  Timeline {
    /// Films and series instead of bangumi
    #[arg(long)]
    drama: bool,
    /// Days back
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(0..=7))]
    before: u8,
    /// Days ahead
    #[arg(long, default_value_t = 6, value_parser = clap::value_parser!(u8).range(0..=7))]
    after: u8,
    #[command(flatten)]
    page: PageArgs,
  },
}

async fn run_pgc(api: &Api, command: PgcCommand) -> Result<Data> {
  use PgcCommand as E;
  Ok(match command {
    E::Bangumi { season, info, page } => {
      let season = api.ctx.collection_ref(&season)?;
      let v = bangumi::season(api, &season).await?;
      if info {
        Data::Collections(atp_core::Page::last(vec![bangumi::info(&v)]))
      } else {
        let episodes = bangumi::episodes(&v);
        Data::Posts(
          page
            .collect(async |_| Ok(atp_core::Page::last(episodes.clone())))
            .await?,
        )
      }
    }
    E::BangumiFollow { season, undo } => {
      let season = api.ctx.collection_ref(&season)?;
      Data::Action(bangumi::follow(api, &season, undo).await?)
    }
    E::MyBangumi { user, drama, page } => {
      let user = user.map(|u| api.ctx.user_ref(&u)).transpose()?;
      Data::Collections(
        page
          .collect(async |r| bangumi::followed(api, user.as_deref(), drama, &r).await)
          .await?,
      )
    }
    E::Timeline {
      drama,
      before,
      after,
      page,
    } => Data::Posts(
      page
        .collect_dated(async |_| bangumi::timeline(api, drama, before, after).await)
        .await?,
    ),
  })
}

/// Live rooms.
#[derive(Debug, clap::Subcommand)]
pub enum RoomCommand {
  /// A live room: title, streamer, live state, audience (room id or link, or uid:MID / @name)
  #[command(after_long_help = EX_LIVE)]
  Live {
    /// Room: number, `live:ROOM`, live.bilibili.com link, or the streamer as `uid:MID` / `@name`
    room: String,
  },
  /// Live rooms of the people you follow
  LiveFollowing {
    #[command(flatten)]
    page: PageArgs,
  },
}

async fn run_rooms(api: &Api, command: RoomCommand) -> Result<Data> {
  use RoomCommand as E;
  Ok(match command {
    E::Live { room } => Data::Post(Box::new(rooms::read(api, &room).await?)),
    E::LiveFollowing { page } => Data::Posts(
      page
        .collect(async |r| rooms::following(api, &r).await)
        .await?,
    ),
  })
}

/// What users keep in their space: lists, articles, coined and liked videos.
#[derive(Debug, clap::Subcommand)]
pub enum SpaceCommand {
  /// Collections (合集) and series (系列) of a user
  Lists {
    /// User: mid, `uid:MID`, space link, name or #N
    user: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Videos of a collection or series (its link, MID/ID or MID/series/ID)
  #[command(after_long_help = EX_LIST)]
  List {
    /// Collection: space link, `MID/ID` (合集), `MID/series/ID` (系列) or #N
    list: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Column articles (专栏) of a user
  Articles {
    /// User: mid, `uid:MID`, space link, name or #N
    user: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Videos a user recently gave coins to (when they make it public)
  Coined {
    /// User: mid, `uid:MID`, space link, name or #N
    user: String,
  },
  /// Videos a user recently liked (when they make it public)
  Liked {
    /// User: mid, `uid:MID`, space link, name or #N
    user: String,
  },
}

const EX_TOPIC: &str = "\
Examples:
  atp bili search 原神 -t topic
  atp bili topic '#1' --sort new

More: atp guide bili topic";

const EX_BANGUMI: &str = "\
Examples:
  atp bili bangumi ss28747
  atp bili bangumi ep508404 --info
  atp bili bangumi 'https://www.bilibili.com/bangumi/media/md28229899'

More: atp guide bili bangumi";

const EX_LIVE: &str = "\
Examples:
  atp bili live 22637261
  atp bili live uid:672328094
  atp bili live https://live.bilibili.com/22637261";

const EX_LIST: &str = "\
Examples:
  atp bili lists 946974
  atp bili list '#1'
  atp bili list 946974/1888
  atp bili list 946974/series/250285";

async fn run_spaces(api: &Api, command: SpaceCommand) -> Result<Data> {
  use SpaceCommand as E;
  Ok(match command {
    E::Lists { user, page } => {
      let user = api.ctx.user_ref(&user)?;
      Data::Collections(
        page
          .collect(async |r| lists::lists(api, &user, &r).await)
          .await?,
      )
    }
    E::List { list, page } => {
      let list = api.ctx.collection_ref(&list)?;
      Data::Posts(
        page
          .collect_dated(async |r| lists::videos(api, &list, &r).await)
          .await?,
      )
    }
    E::Articles { user, page } => {
      let user = api.ctx.user_ref(&user)?;
      Data::Posts(
        page
          .collect_dated(async |r| articles::by_user(api, &user, &r).await)
          .await?,
      )
    }
    E::Coined { user } => Data::Posts(videos::recent(api, &api.ctx.user_ref(&user)?, false).await?),
    E::Liked { user } => Data::Posts(videos::recent(api, &api.ctx.user_ref(&user)?, true).await?),
  })
}

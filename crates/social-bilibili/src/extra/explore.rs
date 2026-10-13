//! Discovery and reading: trends, weekly picks, topics, bangumi, live rooms
//! and a user's collections / articles.

use social_core::cli::PageArgs;
use social_core::{Ctx, Data, Result};

use crate::{discover, live, pgc, space};

#[derive(Debug, clap::Subcommand)]
pub enum Explore {
  /// Trending searches (热搜)
  Trending {
    #[arg(short = 'n', long, default_value_t = 20)]
    limit: usize,
  },
  /// One issue of the weekly picks (每周必看); the latest by default
  Weekly {
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
  Topic {
    topic: String,
    #[arg(long, value_parser = ["hot", "new"])]
    sort: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Episodes of a bangumi / film / documentary (ss…, ep…, md… or link)
  Bangumi {
    season: String,
    /// The season itself: rating, status, counters
    #[arg(long)]
    info: bool,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Follow (追番 / 追剧) a bangumi or film, or stop with --undo
  BangumiFollow {
    season: String,
    #[arg(long)]
    undo: bool,
  },
  /// Bangumi a user follows (yours when USER is omitted); --drama for films and series
  MyBangumi {
    user: Option<String>,
    #[arg(long)]
    drama: bool,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Episodes airing around today (新番时间表)
  Timeline {
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
  /// A live room: title, streamer, live state, audience (room id or link, or uid:MID / @name)
  Live { room: String },
  /// Live rooms of the people you follow
  LiveFollowing {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Collections (合集) and series (系列) of a user
  Lists {
    user: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Videos of a collection or series (its link, MID/ID or MID/series/ID)
  List {
    list: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Column articles (专栏) of a user
  Articles {
    user: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Videos a user recently gave coins to (when they make it public)
  Coined { user: String },
  /// Videos a user recently liked (when they make it public)
  Liked { user: String },
}

pub async fn run(ctx: &Ctx, command: Explore) -> Result<Data> {
  use Explore as E;
  Ok(match command {
    E::Trending { limit } => Data::Collections(discover::trending(ctx, limit).await?),
    E::Weekly { number, page } => Data::Posts(
      page
        .collect_dated(async |_| discover::weekly(ctx, number).await)
        .await?,
    ),
    E::Precious { page } => Data::Posts(
      page
        .collect_dated(async |_| discover::precious(ctx).await)
        .await?,
    ),
    E::Topic { topic, sort, page } => {
      let topic = ctx.collection_ref(&topic)?;
      Data::Posts(
        page
          .collect_dated(async |r| discover::topic(ctx, &topic, sort.as_deref(), &r).await)
          .await?,
      )
    }
    E::Bangumi { season, info, page } => {
      let season = ctx.collection_ref(&season)?;
      let v = pgc::season(ctx, &season).await?;
      if info {
        Data::Collections(social_core::Page::last(vec![pgc::info(&v)]))
      } else {
        let episodes = pgc::episodes(&v);
        Data::Posts(
          page
            .collect(async |_| Ok(social_core::Page::last(episodes.clone())))
            .await?,
        )
      }
    }
    E::BangumiFollow { season, undo } => {
      let season = ctx.collection_ref(&season)?;
      Data::Action(pgc::follow(ctx, &season, undo).await?)
    }
    E::MyBangumi { user, drama, page } => {
      let user = user.map(|u| ctx.user_ref(&u)).transpose()?;
      Data::Collections(
        page
          .collect(async |r| pgc::followed(ctx, user.as_deref(), drama, &r).await)
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
        .collect_dated(async |_| pgc::timeline(ctx, drama, before, after).await)
        .await?,
    ),
    E::Live { room } => Data::Post(Box::new(live::read(ctx, &room).await?)),
    E::LiveFollowing { page } => Data::Posts(
      page
        .collect(async |r| live::following(ctx, &r).await)
        .await?,
    ),
    E::Lists { user, page } => {
      let user = ctx.user_ref(&user)?;
      Data::Collections(
        page
          .collect(async |r| space::lists(ctx, &user, &r).await)
          .await?,
      )
    }
    E::List { list, page } => {
      let list = ctx.collection_ref(&list)?;
      Data::Posts(
        page
          .collect_dated(async |r| space::videos(ctx, &list, &r).await)
          .await?,
      )
    }
    E::Articles { user, page } => {
      let user = ctx.user_ref(&user)?;
      Data::Posts(
        page
          .collect_dated(async |r| space::articles(ctx, &user, &r).await)
          .await?,
      )
    }
    E::Coined { user } => Data::Posts(space::recent(ctx, &ctx.user_ref(&user)?, false).await?),
    E::Liked { user } => Data::Posts(space::recent(ctx, &ctx.user_ref(&user)?, true).await?),
  })
}

//! Bangumi commands beyond the shared set.

use atp_core::cli::{PageArgs, read_text};
use atp_core::{Data, Result};

use crate::api::Api;
use crate::subjects;
use crate::users::{self, Mark};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Characters of a subject, with their voice actors
  Characters {
    /// Subject: link, id or #N
    subject: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Staff of a subject (original work, director, music …)
  Persons {
    /// Subject: link, id or #N
    subject: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Subjects related to a subject: sequels, adaptations, soundtracks …
  Relations {
    /// Subject: link, id or #N
    subject: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Episodes of a subject
  Episodes {
    /// Subject: link, id or #N
    subject: String,
    /// Only one kind: main, sp, op, ed, pv, mad, other
    #[arg(short = 't', long = "type")]
    kind: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Subjects of one type by rank or date, as on the browse pages
  #[command(after_long_help = EX_BROWSE)]
  Browse {
    /// anime, book, music, game or real
    kind: String,
    /// Order: rank (best first) or date (newest first)
    #[arg(short, long, default_value = "rank")]
    sort: String,
    /// Only subjects of this year
    #[arg(long)]
    year: Option<u32>,
    /// Only subjects of this month (with --year)
    #[arg(long)]
    month: Option<u32>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// A user's collection (收藏), the most recently changed first
  #[command(after_long_help = EX_LIBRARY)]
  Library {
    /// User: name, link or #N; yours (needs a token) when omitted
    user: Option<String>,
    /// Only one subject type: anime, book, music, game, real
    #[arg(short = 't', long = "type")]
    kind: Option<String>,
    /// Only one state: wish, done, doing, on-hold, dropped
    #[arg(long)]
    status: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Put a subject in your collection or change its entry (needs a token)
  #[command(after_long_help = EX_MARK)]
  Mark {
    /// Subject: link, id or #N
    subject: String,
    /// State: wish, done, doing, on-hold or dropped
    status: String,
    /// Your rating, 1 to 10 (0 removes it)
    #[arg(short, long)]
    rating: Option<u8>,
    /// Your short review; `-` reads it from stdin
    #[arg(short, long)]
    comment: Option<String>,
    /// A tag (repeatable; no spaces)
    #[arg(long = "tag")]
    tags: Vec<String>,
    /// Only you can see the entry
    #[arg(long)]
    private: bool,
  },
  /// Record how far you are in a subject (needs a token)
  #[command(after_long_help = EX_PROGRESS)]
  Progress {
    /// Subject: link, id or #N
    subject: String,
    /// The last episode watched (marks main episodes 1 to N), or the
    /// chapters read of a book
    episode: Option<u32>,
    /// Volumes read (books)
    #[arg(long = "vol")]
    volume: Option<u32>,
  },
}

const EX_BROWSE: &str = "\
Examples:
  atp bangumi browse anime -n 20              # the best-ranked anime
  atp bangumi browse anime --sort date --year 2026 --month 10
  atp bgm browse book -n 50 -f csv > books.csv

More: atp guide bangumi browse";

const EX_LIBRARY: &str = "\
Examples:
  atp bangumi library sai --type anime --status done -n 100
  atp bangumi library --status doing          # yours

More: atp guide bangumi library";

const EX_MARK: &str = "\
Examples:
  atp bangumi mark 400602 doing
  atp bangumi mark '#1' done --rating 9 --comment '旅途的意义' --tag 治愈
  atp bangumi mark https://bgm.tv/subject/400602 wish --private

More: atp guide bangumi mark";

const EX_PROGRESS: &str = "\
Examples:
  atp bangumi progress 400602 12              # watched episodes 1 to 12
  atp bangumi progress <book> 30 --vol 3      # 30 chapters, 3 volumes read

More: atp guide bangumi progress";

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    Command::Characters { subject, page } => linked(api, &subject, "characters", page).await?,
    Command::Persons { subject, page } => linked(api, &subject, "persons", page).await?,
    Command::Relations { subject, page } => linked(api, &subject, "subjects", page).await?,
    Command::Episodes {
      subject,
      kind,
      page,
    } => episodes(api, &subject, kind.as_deref(), page).await?,
    Command::Browse {
      kind,
      sort,
      year,
      month,
      page,
    } => Data::Posts(
      page
        .collect_dated(async |req| subjects::browse(api, &kind, &sort, year, month, &req).await)
        .await?,
    ),
    Command::Library {
      user,
      kind,
      status,
      page,
    } => library(api, user, kind, status, page).await?,
    Command::Mark {
      subject,
      status,
      rating,
      comment,
      tags,
      private,
    } => {
      let mark = Mark {
        status,
        rating,
        comment: comment.map(|c| read_text(Some(c))).transpose()?,
        tags,
        private,
      };
      Data::Action(users::mark(api, &ctx.post_ref(&subject)?, mark).await?)
    }
    Command::Progress {
      subject,
      episode,
      volume,
    } => Data::Action(users::progress(api, &ctx.post_ref(&subject)?, episode, volume).await?),
  })
}

async fn episodes(api: &Api, subject: &str, kind: Option<&str>, page: PageArgs) -> Result<Data> {
  let subject = api.ctx.post_ref(subject)?;
  Ok(Data::Posts(
    page
      .collect_dated(async |req| subjects::episodes(api, &subject, kind, &req).await)
      .await?,
  ))
}

async fn linked(api: &Api, subject: &str, what: &str, page: PageArgs) -> Result<Data> {
  let subject = api.ctx.post_ref(subject)?;
  Ok(Data::Posts(
    page
      .collect_dated(async |req| subjects::linked(api, &subject, what, &req).await)
      .await?,
  ))
}

async fn library(
  api: &Api,
  user: Option<String>,
  kind: Option<String>,
  status: Option<String>,
  page: PageArgs,
) -> Result<Data> {
  let user = user.map(|u| api.ctx.user_ref(&u)).transpose()?;
  let (user, kind, status) = (user.as_deref(), kind.as_deref(), status.as_deref());
  Ok(Data::Posts(
    page
      .collect_dated(async |req| users::library(api, user, kind, status, &req).await)
      .await?,
  ))
}

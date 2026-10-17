//! Douban commands beyond the shared set.

use atp_core::cli::PageArgs;
use atp_core::{Data, Result};

use crate::api::Api;
use crate::{contacts, groups, subjects, users};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Reviews (影评 / 书评 / 乐评) of a film, show, book or album
  #[command(after_long_help = EX_REVIEWS)]
  Reviews {
    /// Subject: link, `book:ID` … or #N
    subject: String,
    /// Order: hot (most useful first) or latest
    #[arg(short, long, default_value = "hot")]
    sort: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Topics of a group (小组), pinned ones first
  #[command(after_long_help = EX_GROUP)]
  Group {
    /// Group: link, id, name in its link, or #N
    group: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Any Douban list (榜单 / 片单 of the `subject_collection` kind), in order
  #[command(after_long_help = EX_LIST)]
  List {
    /// The list's `m.douban.com/subject_collection/…` link or its id
    list: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// What a user marked (想看 / 在看 / 看过 …), the latest first
  #[command(after_long_help = EX_LIBRARY)]
  Library {
    /// User: link, uid, number or #N; yours (needs a login) when omitted
    user: Option<String>,
    /// movie (films and shows), book or music
    #[arg(short = 't', long = "type", default_value = "movie")]
    kind: String,
    /// wish (想看), doing (在看 / 在读) or done (看过 / 读过)
    #[arg(long, default_value = "done")]
    status: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Mark a subject wish / doing / done, with stars (needs a login)
  #[command(after_long_help = EX_MARK)]
  Mark {
    /// Subject: link, `book:ID` … or #N
    subject: String,
    /// wish (想看), doing (在看 / 在读; not for films) or done (看过 / 读过)
    status: String,
    /// Your stars, 1 to 5 (doing and done only)
    #[arg(short, long)]
    rating: Option<u8>,
  },
  /// Set the remark (备注) you see for someone you follow (needs a login)
  Remark {
    /// User: /people/ link, uid, user number or #N
    user: String,
    /// The remark; empty clears it
    text: String,
  },
}

const EX_REVIEWS: &str = "\
Examples:
  atp douban reviews https://movie.douban.com/subject/1292052/
  atp douban reviews book:1007305 --sort latest -n 50

More: atp guide douban reviews";

const EX_GROUP: &str = "\
Examples:
  atp douban group 54359
  atp douban group https://www.douban.com/group/54359/ -n 50
  atp douban read '#1'                         # one topic
  atp douban comments '#1'                     # its replies

More: atp guide douban group";

const EX_LIST: &str = "\
Examples:
  atp douban list movie_top250 -n 250 -f csv > top250.csv
  atp douban list https://m.douban.com/subject_collection/ECDIHUN4A

More: atp guide douban list";

const EX_LIBRARY: &str = "\
Examples:
  atp douban library ahbei --type movie --status done -n 100
  atp douban library --type book --status wish  # yours

More: atp guide douban library";

const EX_MARK: &str = "\
Examples:
  atp douban mark https://movie.douban.com/subject/1292052/ done --rating 5
  atp douban mark book:1007305 wish

More: atp guide douban mark";

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    Command::Reviews {
      subject,
      sort,
      page,
    } => {
      let subject = ctx.post_ref(&subject)?;
      Data::Posts(
        page
          .collect_dated(async |req| subjects::reviews(api, &subject, Some(&sort), &req).await)
          .await?,
      )
    }
    Command::Group { group, page } => {
      let group = ctx.post_ref(&group)?;
      Data::Posts(
        page
          .collect_dated(async |req| groups::topics(api, &group, &req).await)
          .await?,
      )
    }
    Command::List { list, page } => {
      let id = subjects::list_id(&list)?;
      Data::Posts(
        page
          .collect_dated(async |req| subjects::list(api, &id, &req).await)
          .await?,
      )
    }
    Command::Library {
      user,
      kind,
      status,
      page,
    } => library(api, user, &kind, &status, page).await?,
    Command::Mark {
      subject,
      status,
      rating,
    } => Data::Action(users::mark(api, &ctx.post_ref(&subject)?, &status, rating).await?),
    Command::Remark { user, text } => {
      Data::Action(contacts::remark(api, &ctx.user_ref(&user)?, text.trim()).await?)
    }
  })
}

async fn library(
  api: &Api,
  user: Option<String>,
  kind: &str,
  status: &str,
  page: PageArgs,
) -> Result<Data> {
  let user = user.map(|u| api.ctx.user_ref(&u)).transpose()?;
  let user = user.as_deref();
  Ok(Data::Posts(
    page
      .collect_dated(async |req| users::library(api, user, kind, status, &req).await)
      .await?,
  ))
}

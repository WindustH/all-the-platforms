//! WeRead commands beyond the shared set.

use std::path::PathBuf;

use atp_core::cli::PageArgs;
use atp_core::paging::slice;
use atp_core::{Data, Result};

use crate::api::Api;
use crate::{books, notes, shelf};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// Your highlights (划线) and thoughts (想法) in a book, in reading order;
  /// without BOOK, the books you annotated
  #[command(after_long_help = EX_NOTES)]
  Notes {
    /// Book: id, reader or detail link, or #N; omit it to list annotated books
    book: Option<String>,
    /// Write the notes as Markdown to this file instead of listing them
    #[arg(long, value_name = "FILE", requires = "book")]
    markdown: Option<PathBuf>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// How far you are in a book and how long you have read it
  #[command(after_long_help = EX_PROGRESS)]
  Progress {
    /// Book: id, reader or detail link, or #N
    book: String,
  },
  /// Totals of your shelf and notes: books, finished, started, notes
  Stats,
  /// Books of a store category or ranking by its id, in order
  #[command(after_long_help = EX_CATEGORY)]
  Category {
    /// Category id (`100000` 精品小说, `200000` 历史 …) or ranking id (`rising` …)
    id: String,
    #[command(flatten)]
    page: PageArgs,
  },
}

const EX_NOTES: &str = "\
Examples:
  atp weread notes                             # books you annotated
  atp weread notes '#1'                        # highlights and thoughts
  atp weread notes 695233 --markdown 三体.md
  atp weread notes https://weread.qq.com/web/bookDetail/ce032b305a9bc1ce0b0dd2a -f json

More: atp guide weread notes";

const EX_PROGRESS: &str = "\
Examples:
  atp weread favorites                         # your shelf
  atp weread progress '#1'

More: atp guide weread progress";

const EX_CATEGORY: &str = "\
Examples:
  atp weread category 100000 -n 50             # 精品小说
  atp weread category 700000                   # 计算机

More: atp guide weread category";

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  let ctx = &api.ctx;
  Ok(match command {
    Command::Notes {
      book: None, page, ..
    } => Data::Posts(
      page
        .collect_dated(async |req| notes::notebooks(api, &req).await)
        .await?,
    ),
    Command::Notes {
      book: Some(book),
      markdown,
      page,
    } => {
      let found = notes::notes(api, &ctx.post_ref(&book)?).await?;
      if let Some(path) = markdown {
        return notes::export(&found, &path);
      }
      let items = notes::posts(found);
      Data::Posts(
        page
          .collect_dated(async |req| Ok(slice(items.clone(), &req)))
          .await?,
      )
    }
    Command::Progress { book } => Data::Value(shelf::progress(api, &ctx.post_ref(&book)?).await?),
    Command::Stats => Data::Value(shelf::stats(api).await?),
    Command::Category { id, page } => Data::Posts(
      page
        .collect_dated(async |req| books::category_books(api, &id, &req).await)
        .await?,
    ),
  })
}

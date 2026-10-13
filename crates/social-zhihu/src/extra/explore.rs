//! Discovery: followers of questions and topics, columns, hot searches and
//! Zhihu Daily.

use social_core::cli::PageArgs;
use social_core::{Data, Page, Result};

use crate::api::Api;
use crate::refs::Target;
use crate::{discover, lists, refs};

#[derive(Debug, clap::Subcommand)]
pub enum ExploreCommand {
  /// Who follows a question
  QuestionFollowers {
    /// Question id, q:<id> or URL
    question: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Who follows a topic
  TopicFollowers {
    /// Topic id or URL
    topic: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Articles of a column (专栏), newest first
  Column {
    /// Column id or URL (`https://zhuanlan.zhihu.com/<id>`)
    column: String,
    /// Show the column itself instead of its articles
    #[arg(long)]
    info: bool,
    #[command(flatten)]
    page: PageArgs,
  },
  /// What people search for right now (热搜)
  HotSearch,
  /// Zhihu Daily (知乎日报) stories, newest day first; no login needed
  Daily {
    /// Start at this day (YYYYMMDD)
    #[arg(long)]
    date: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
}

pub async fn run(api: &Api, command: ExploreCommand) -> Result<Data> {
  match command {
    ExploreCommand::QuestionFollowers { question, page } => {
      let id = Target::question(&api.ctx.post_ref(&question)?)?;
      let users = page.collect(async |r| lists::question_followers(api, &id, &r).await);
      Ok(Data::Users(users.await?))
    }
    ExploreCommand::TopicFollowers { topic, page } => {
      let id = refs::topic_id(&api.ctx.collection_ref(&topic)?)?;
      let users = page.collect(async |r| lists::topic_followers(api, &id, &r).await);
      Ok(Data::Users(users.await?))
    }
    ExploreCommand::Column { column, info, page } => {
      let id = refs::column_id(&api.ctx.collection_ref(&column)?)?;
      if info {
        let column = lists::column(api, &id).await?;
        return Ok(Data::Collections(Page::last(vec![column])));
      }
      let posts = page.collect_dated(async |r| lists::column_items(api, &id, &r).await);
      Ok(Data::Posts(posts.await?))
    }
    ExploreCommand::HotSearch => Ok(Data::Collections(discover::hot_search(api).await?)),
    ExploreCommand::Daily { date, page } => {
      let date = date.as_deref();
      let posts = page.collect(async |r| discover::daily(api, date, &r).await);
      Ok(Data::Posts(posts.await?))
    }
  }
}

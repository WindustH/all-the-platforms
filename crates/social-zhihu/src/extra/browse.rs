//! Browsing: a topic, and the lists of a user (articles, pins, activities,
//! questions, videos, columns, follows).

use social_core::cli::PageArgs;
use social_core::{Data, Page, Result};

use super::owner;
use crate::api::Api;
use crate::{feeds, lists, refs, topics, users};

#[derive(Debug, clap::Subcommand)]
pub enum BrowseCommand {
  /// Show a topic, or list its essence (精华) posts
  Topic {
    id: String,
    /// List the topic's essence posts instead
    #[arg(long)]
    essence: bool,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Articles of a user
  UserArticles {
    user: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Pins (想法) of a user
  UserPins {
    user: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// What a user did lately: answered, posted, voted, followed (动态); yours without USER
  Activities {
    user: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Questions a user asked; yours without USER
  UserQuestions {
    user: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Videos of a user; yours without USER
  UserVideos {
    user: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Columns (专栏) a user writes in; yours without USER
  UserColumns {
    user: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Questions, topics, columns or favorites folders a user follows; yours without USER
  UserFollowing {
    user: Option<String>,
    /// What to list
    #[arg(short = 't', long = "type", default_value = "questions", value_parser = ["questions", "topics", "columns", "folders"])]
    kind: String,
    #[command(flatten)]
    page: PageArgs,
  },
}

pub async fn run(api: &Api, command: BrowseCommand) -> Result<Data> {
  match command {
    BrowseCommand::Topic { id, essence, page } => {
      let id = refs::topic_id(&api.ctx.collection_ref(&id)?)?;
      if essence {
        let posts = page.collect_dated(async |r| topics::topic_essence(api, &id, &r).await);
        Ok(Data::Posts(posts.await?))
      } else {
        let topic = topics::topic(api, &id).await?;
        Ok(Data::Collections(Page::last(vec![topic])))
      }
    }
    BrowseCommand::UserArticles { user, page } => {
      let token = refs::user(&api.ctx.user_ref(&user)?)?;
      let posts = page.collect_dated(async |r| users::articles(api, &token, &r).await);
      Ok(Data::Posts(posts.await?))
    }
    BrowseCommand::UserPins { user, page } => {
      let token = refs::user(&api.ctx.user_ref(&user)?)?;
      let posts = page.collect_dated(async |r| users::pins(api, &token, &r).await);
      Ok(Data::Posts(posts.await?))
    }
    BrowseCommand::Activities { user, page } => {
      let token = owner(api, user).await?;
      let posts = page.collect_dated(async |r| feeds::activities(api, &token, &r).await);
      Ok(Data::Posts(posts.await?))
    }
    BrowseCommand::UserQuestions { user, page } => {
      let token = owner(api, user).await?;
      let posts = page.collect_dated(async |r| lists::questions(api, &token, &r).await);
      Ok(Data::Posts(posts.await?))
    }
    BrowseCommand::UserVideos { user, page } => {
      let token = owner(api, user).await?;
      let posts = page.collect_dated(async |r| lists::videos(api, &token, &r).await);
      Ok(Data::Posts(posts.await?))
    }
    BrowseCommand::UserColumns { user, page } => {
      let token = owner(api, user).await?;
      let columns = page.collect(async |r| lists::columns(api, &token, &r).await);
      Ok(Data::Collections(columns.await?))
    }
    BrowseCommand::UserFollowing { user, kind, page } => {
      let token = owner(api, user).await?;
      if kind == "questions" {
        let posts = page.collect(async |r| lists::following_questions(api, &token, &r).await);
        return Ok(Data::Posts(posts.await?));
      }
      let items = page.collect(async |r| lists::following(api, &token, &kind, &r).await);
      Ok(Data::Collections(items.await?))
    }
  }
}

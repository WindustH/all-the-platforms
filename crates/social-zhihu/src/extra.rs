//! Zhihu-only commands.

use std::path::PathBuf;

use social_core::cli::PageArgs;
use social_core::{Data, Error, Page, Result};

use crate::api::Api;
use crate::refs::Target;
use crate::{
  comments, discover, feeds, insights, library, lists, posts, publish, refs, topics, users,
};

#[derive(Debug, clap::Subcommand)]
pub enum Extra {
  /// Answers of a question
  Answers {
    /// Question id, q:<id> or URL
    question: String,
    /// Answer order
    #[arg(long, default_value = "default", value_parser = ["default", "created"])]
    sort: String,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Ask a question (提问)
  Ask {
    title: String,
    /// Question description
    #[arg(short, long, default_value = "")]
    detail: String,
    /// Topic id (repeatable)
    #[arg(short = 't', long = "topic", value_name = "TOPIC")]
    topics: Vec<String>,
    /// Attach an image (repeatable)
    #[arg(short = 'i', long = "image", value_name = "PATH")]
    images: Vec<PathBuf>,
  },
  /// Answer a question (回答)
  Answer {
    /// Question id, q:<id> or URL
    question: String,
    /// Body text; `-` reads it from stdin
    body: String,
    /// Read the body as Markdown (headings, lists, quotes, code, tables, inline images)
    #[arg(short, long)]
    markdown: bool,
    /// Attach an image at the end (repeatable)
    #[arg(short = 'i', long = "image", value_name = "PATH")]
    images: Vec<PathBuf>,
  },
  /// Publish a column article (文章)
  Article {
    title: String,
    /// Body text; `-` reads it from stdin
    body: String,
    /// Read the body as Markdown (headings, lists, quotes, code, tables, inline images)
    #[arg(short, long)]
    markdown: bool,
    /// Topic id (repeatable)
    #[arg(short = 't', long = "topic", value_name = "TOPIC")]
    topics: Vec<String>,
    /// Attach an image (repeatable)
    #[arg(short = 'i', long = "image", value_name = "PATH")]
    images: Vec<PathBuf>,
  },
  /// Follow a question
  FollowQuestion {
    /// Question id, q:<id> or URL
    question: String,
    /// Unfollow instead
    #[arg(long)]
    undo: bool,
  },
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
  /// Vote on an answer: up, down (反对) or neutral
  Vote {
    post: String,
    #[arg(value_parser = ["up", "down", "neutral"])]
    direction: String,
  },
  /// Like a comment, or take the like back
  LikeComment {
    /// Comment id
    comment: String,
    #[arg(long)]
    undo: bool,
  },
  /// Block a user (屏蔽), or unblock with --undo
  Block {
    user: String,
    #[arg(long)]
    undo: bool,
  },
  /// Create a favorites folder (收藏夹)
  CreateFolder {
    title: String,
    #[arg(short, long, default_value = "")]
    description: String,
    /// Only you can see it
    #[arg(long)]
    private: bool,
  },
  /// Delete one of your favorites folders
  DeleteFolder {
    /// Folder id (see `collections`)
    folder: String,
    #[arg(short, long)]
    yes: bool,
  },
  /// Your own posts with their lifetime numbers from the creator center (内容分析)
  Creations {
    /// Content type
    #[arg(short = 't', long = "type", default_value = "answer", value_parser = ["answer", "article", "pin"])]
    kind: String,
    #[command(flatten)]
    page: PageArgs,
  },
}

pub async fn run(api: &Api, command: Extra) -> Result<Data> {
  match command {
    Extra::Answers {
      question,
      sort,
      page,
    } => {
      let id = Target::question(&api.ctx.post_ref(&question)?)?;
      let posts = page.collect_dated(async |r| posts::answers(api, &id, &sort, &r).await);
      Ok(Data::Posts(posts.await?))
    }
    Extra::Ask {
      title,
      detail,
      topics,
      images,
    } => {
      check(&title, &images)?;
      let done = publish::question(api, title.trim(), &detail, &topics, &images);
      Ok(Data::Action(done.await?))
    }
    Extra::Answer {
      question,
      body,
      markdown,
      images,
    } => {
      let id = Target::question(&api.ctx.post_ref(&question)?)?;
      let body = read_body(body, "the answer")?;
      check("answer", &images)?;
      let done = publish::answer(api, &id, &body, markdown, &images);
      Ok(Data::Action(done.await?))
    }
    Extra::Article {
      title,
      body,
      markdown,
      topics,
      images,
    } => {
      let body = read_body(body, "the article body")?;
      check(&title, &images)?;
      let done = publish::article(api, title.trim(), &body, markdown, &topics, &images);
      Ok(Data::Action(done.await?))
    }
    Extra::FollowQuestion { question, undo } => {
      let id = Target::question(&api.ctx.post_ref(&question)?)?;
      Ok(Data::Action(
        users::follow(api, "questions", &id, undo).await?,
      ))
    }
    Extra::Topic { id, essence, page } => {
      let id = topic_id(&api.ctx.collection_ref(&id)?)?;
      if essence {
        let posts = page.collect_dated(async |r| topics::topic_essence(api, &id, &r).await);
        Ok(Data::Posts(posts.await?))
      } else {
        let topic = topics::topic(api, &id).await?;
        Ok(Data::Collections(Page::last(vec![topic])))
      }
    }
    Extra::UserArticles { user, page } => {
      let token = refs::user(&api.ctx.user_ref(&user)?)?;
      let posts = page.collect_dated(async |r| users::articles(api, &token, &r).await);
      Ok(Data::Posts(posts.await?))
    }
    Extra::UserPins { user, page } => {
      let token = refs::user(&api.ctx.user_ref(&user)?)?;
      let posts = page.collect_dated(async |r| users::pins(api, &token, &r).await);
      Ok(Data::Posts(posts.await?))
    }
    Extra::Activities { user, page } => {
      let token = owner(api, user).await?;
      let posts = page.collect_dated(async |r| feeds::activities(api, &token, &r).await);
      Ok(Data::Posts(posts.await?))
    }
    Extra::UserQuestions { user, page } => {
      let token = owner(api, user).await?;
      let posts = page.collect_dated(async |r| lists::questions(api, &token, &r).await);
      Ok(Data::Posts(posts.await?))
    }
    Extra::UserVideos { user, page } => {
      let token = owner(api, user).await?;
      let posts = page.collect_dated(async |r| lists::videos(api, &token, &r).await);
      Ok(Data::Posts(posts.await?))
    }
    Extra::UserColumns { user, page } => {
      let token = owner(api, user).await?;
      let columns = page.collect(async |r| lists::columns(api, &token, &r).await);
      Ok(Data::Collections(columns.await?))
    }
    Extra::UserFollowing { user, kind, page } => {
      let token = owner(api, user).await?;
      if kind == "questions" {
        let posts = page.collect(async |r| lists::following_questions(api, &token, &r).await);
        return Ok(Data::Posts(posts.await?));
      }
      let items = page.collect(async |r| lists::following(api, &token, &kind, &r).await);
      Ok(Data::Collections(items.await?))
    }
    Extra::QuestionFollowers { question, page } => {
      let id = Target::question(&api.ctx.post_ref(&question)?)?;
      let users = page.collect(async |r| lists::question_followers(api, &id, &r).await);
      Ok(Data::Users(users.await?))
    }
    Extra::TopicFollowers { topic, page } => {
      let id = topic_id(&api.ctx.collection_ref(&topic)?)?;
      let users = page.collect(async |r| lists::topic_followers(api, &id, &r).await);
      Ok(Data::Users(users.await?))
    }
    Extra::Column { column, info, page } => {
      let id = column_id(&api.ctx.collection_ref(&column)?)?;
      if info {
        let column = lists::column(api, &id).await?;
        return Ok(Data::Collections(Page::last(vec![column])));
      }
      let posts = page.collect_dated(async |r| lists::column_items(api, &id, &r).await);
      Ok(Data::Posts(posts.await?))
    }
    Extra::HotSearch => Ok(Data::Collections(discover::hot_search(api).await?)),
    Extra::Daily { date, page } => {
      let date = date.as_deref();
      let posts = page.collect(async |r| discover::daily(api, date, &r).await);
      Ok(Data::Posts(posts.await?))
    }
    Extra::Vote { post, direction } => {
      let target = Target::parse(&api.ctx.post_ref(&post)?)?;
      Ok(Data::Action(posts::vote(api, &target, &direction).await?))
    }
    Extra::LikeComment { comment, undo } => Ok(Data::Action(
      comments::like_comment(api, comment.trim(), undo).await?,
    )),
    Extra::Block { user, undo } => {
      let token = refs::user(&api.ctx.user_ref(&user)?)?;
      Ok(Data::Action(users::block(api, &token, undo).await?))
    }
    Extra::CreateFolder {
      title,
      description,
      private,
    } => {
      check(&title, &[])?;
      let done = library::create_folder(api, title.trim(), &description, private);
      Ok(Data::Action(done.await?))
    }
    Extra::DeleteFolder { folder, yes } => {
      if !yes {
        return Err(Error::input(format!(
          "refusing to delete folder {folder} without --yes"
        )));
      }
      let folder = api.ctx.collection_ref(&folder)?;
      Ok(Data::Action(library::delete_folder(api, &folder).await?))
    }
    Extra::Creations { kind, page } => {
      let posts = page.collect_dated(async |r| insights::creations(api, &kind, &r).await);
      Ok(Data::Posts(posts.await?))
    }
  }
}

/// The body argument, or standard input for `-`; empty bodies are refused.
fn read_body(body: String, what: &str) -> Result<String> {
  let body = if body == "-" {
    std::io::read_to_string(std::io::stdin())?
  } else {
    body
  };
  if body.trim().is_empty() {
    return Err(Error::input(format!("{what} is empty")));
  }
  Ok(body)
}

fn check(title: &str, images: &[PathBuf]) -> Result<()> {
  if title.trim().is_empty() {
    return Err(Error::input("the title is empty"));
  }
  match images.iter().find(|i| !i.is_file()) {
    Some(missing) => Err(Error::input(format!(
      "image not found: {}",
      missing.display()
    ))),
    None => Ok(()),
  }
}

/// A user given as `#N`, token or link; the logged-in account when absent.
async fn owner(api: &Api, user: Option<String>) -> Result<String> {
  let token = match user {
    Some(u) => Some(refs::user(&api.ctx.user_ref(&u)?)?),
    None => None,
  };
  users::owner(api, token.as_deref()).await
}

/// A column id, bare or from `https://zhuanlan.zhihu.com/<id>` or
/// `https://www.zhihu.com/column/<id>`.
fn column_id(arg: &str) -> Result<String> {
  let arg = arg.trim().trim_end_matches('/');
  let id = ["zhuanlan.zhihu.com/", "/column/"]
    .iter()
    .find_map(|m| arg.split_once(m).map(|(_, rest)| rest))
    .unwrap_or(arg)
    .split(['/', '?', '#'])
    .next()
    .unwrap_or_default();
  if !id.is_empty() && !id.contains(':') && id != "p" {
    Ok(id.to_owned())
  } else {
    Err(Error::input(format!("not a Zhihu column: `{arg}`")))
  }
}

/// A topic id, bare or from `https://www.zhihu.com/topic/<id>`.
fn topic_id(arg: &str) -> Result<String> {
  let id = arg
    .trim()
    .trim_end_matches('/')
    .rsplit("/topic/")
    .next()
    .unwrap_or_default()
    .split(['/', '?', '#'])
    .next()
    .unwrap_or_default();
  if !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) {
    Ok(id.to_owned())
  } else {
    Err(Error::input(format!("not a Zhihu topic: `{arg}`")))
  }
}

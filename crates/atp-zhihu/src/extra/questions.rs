//! Questions, answers and articles: listing answers, asking, answering,
//! writing articles, following questions.

use std::path::PathBuf;

use atp_core::cli::{PageArgs, read_text};
use atp_core::{Data, Error, Result};

use super::title_and_images;
use crate::api::Api;
use crate::refs::Target;
use crate::{posts, publish, users};

#[derive(Debug, clap::Subcommand)]
pub enum QuestionCommand {
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
  #[command(after_long_help = EX_ASK)]
  Ask {
    /// The question
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
  #[command(after_long_help = EX_ANSWER)]
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
  #[command(after_long_help = EX_ARTICLE)]
  Article {
    /// Article title
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
}

const EX_ASK: &str = "\
Examples:
  atp zhihu ask '如何入门 Rust？'
  atp zhihu ask '如何入门 Rust？' -d '有 C++ 基础' -t 19551388 -i shot.png

More: atp guide zhihu ask";

const EX_ANSWER: &str = "\
Examples:
  atp zhihu answer q:123456 '我的回答'
  atp zhihu answer https://www.zhihu.com/question/123456 - -m < answer.md
  atp zhihu answer 123456 '见图' -i chart.png

More: atp guide zhihu answer";

const EX_ARTICLE: &str = "\
Examples:
  atp zhihu article '标题' - -m < post.md
  atp zhihu article '标题' '正文' -t 19551388 -i cover.png

More: atp guide zhihu article";

pub async fn run(api: &Api, command: QuestionCommand) -> Result<Data> {
  match command {
    QuestionCommand::Answers {
      question,
      sort,
      page,
    } => {
      let id = Target::question(&api.ctx.post_ref(&question)?)?;
      let posts = page.collect_dated(async |r| posts::answers(api, &id, &sort, &r).await);
      Ok(Data::Posts(posts.await?))
    }
    QuestionCommand::Ask {
      title,
      detail,
      topics,
      images,
    } => {
      title_and_images(&title, &images)?;
      let done = publish::question(api, title.trim(), &detail, &topics, &images);
      Ok(Data::Action(done.await?))
    }
    QuestionCommand::Answer {
      question,
      body,
      markdown,
      images,
    } => {
      let id = Target::question(&api.ctx.post_ref(&question)?)?;
      let body = body_text(body, "the answer")?;
      title_and_images("answer", &images)?;
      let done = publish::answer(api, &id, &body, markdown, &images);
      Ok(Data::Action(done.await?))
    }
    QuestionCommand::Article {
      title,
      body,
      markdown,
      topics,
      images,
    } => {
      let body = body_text(body, "the article body")?;
      title_and_images(&title, &images)?;
      let done = publish::article(api, title.trim(), &body, markdown, &topics, &images);
      Ok(Data::Action(done.await?))
    }
    QuestionCommand::FollowQuestion { question, undo } => {
      let id = Target::question(&api.ctx.post_ref(&question)?)?;
      Ok(Data::Action(
        users::follow(api, "questions", &id, undo).await?,
      ))
    }
  }
}

/// The body argument, or standard input for `-`; empty bodies are refused.
fn body_text(body: String, what: &str) -> Result<String> {
  let body = read_text(Some(body))?;
  if body.trim().is_empty() {
    return Err(Error::input(format!("{what} is empty")));
  }
  Ok(body)
}

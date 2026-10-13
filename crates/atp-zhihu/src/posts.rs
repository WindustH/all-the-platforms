//! Posts: reading questions, answers, articles and pins, the answers of a
//! question, and voting, liking and deleting.

use atp_core::{Action, Error, Page, PageReq, Post, Result, Value, ValueExt, json};

use crate::api::{Api, V4, WWW, ZHUANLAN};
use crate::refs::Target;
use crate::{api, parse};

const ANSWER_INCLUDE: &str = "content,excerpt,voteup_count,comment_count,thanks_count,favlists_count,visit_count,reaction,created_time,updated_time,author,question";

pub async fn read(api: &Api, target: &Target) -> Result<Post> {
  let (url, include, parse): (String, Option<&str>, fn(&Value) -> Post) = match target {
    Target::Question(id) => return question(api, id).await,
    Target::Answer(id) => (
      format!("{V4}/answers/{id}"),
      Some(ANSWER_INCLUDE),
      parse::answer,
    ),
    Target::Article(id) => (format!("{ZHUANLAN}/articles/{id}"), None, parse::article),
    Target::Pin(id) => (format!("{V4}/pins/{id}"), None, parse::pin),
    Target::Zvideo(id) => (format!("{V4}/zvideos/{id}"), None, parse::zvideo),
  };
  let mut req = api.get(&url);
  if let Some(include) = include {
    req = req.query("include", include);
  }
  Ok(parse(&api.call(req).await?))
}

/// A question, from its page: the v4 endpoint demands `x-zse-96`.
async fn question(api: &Api, id: &str) -> Result<Post> {
  let state = api.page_state(&format!("{WWW}/question/{id}")).await?;
  let question = state.at(&format!("entities.questions.{id}"));
  if question.is_null() {
    return Err(Error::not_found(format!("no question {id}")));
  }
  Ok(parse::question(&parse::snake_keys(question)))
}

/// Answers of a question, sorted by `default` (votes) or `created`.
pub async fn answers(api: &Api, question: &str, sort: &str, page: &PageReq) -> Result<Page<Post>> {
  let offset = page.number_or(0);
  let v = api
    .call(
      api
        .get(&format!("{V4}/questions/{question}/answers"))
        .queries([
          ("include", format!("data[*].{ANSWER_INCLUDE}")),
          ("offset", offset.to_string()),
          ("limit", page.size_within(20).to_string()),
          ("sort_by", sort.to_owned()),
        ]),
    )
    .await?;
  Ok(Page::new(
    v.list("data").iter().map(parse::answer).collect(),
    api::next_param(&v, "offset"),
  ))
}

/// Upvote an answer, like an article or a pin; `undo` goes back to neutral.
pub async fn like(api: &Api, target: &Target, undo: bool) -> Result<Action> {
  match target {
    Target::Question(_) => {
      return Err(Error::input(
        "questions cannot be liked; use `follow-question` instead",
      ));
    }
    Target::Zvideo(_) => return Err(Error::unsupported("like (videos)")),
    _ => {}
  }
  api.prepare_write().await?;
  let req = match target {
    Target::Article(id) => api
      .post(&format!("{V4}/articles/{id}/voters"))
      .json(&json!({ "voting": if undo { 0 } else { 1 } })),
    Target::Pin(id) if undo => api.delete(&format!("{V4}/pins/{id}/voters/up")),
    Target::Pin(id) => api
      .post(&format!("{V4}/pins/{id}/voters/up"))
      .json(&json!({ "not_sync_moments": true })),
    // Answers (questions were rejected above).
    _ => api
      .post(&format!("{V4}/answers/{}/voters", target.id()))
      .json(&json!({ "type": if undo { "neutral" } else { "up" } })),
  };
  api.call(req).await?;
  let action = if undo { "unlike" } else { "like" };
  Ok(Action::done(action, target.url()))
}

/// Vote on an answer: `up`, `down` (反对) or `neutral`. Articles take `up` /
/// `neutral` only, like pins ([`like`]).
pub async fn vote(api: &Api, target: &Target, direction: &str) -> Result<Action> {
  match (target, direction) {
    (Target::Answer(_), "up" | "down" | "neutral") => {}
    (Target::Answer(_), other) => {
      return Err(Error::input(format!(
        "vote `{other}`: use up, down or neutral"
      )));
    }
    (_, "up") => return like(api, target, false).await,
    (_, "neutral") => return like(api, target, true).await,
    _ => {
      return Err(Error::input(
        "only answers take down votes; articles and pins take up or neutral",
      ));
    }
  }
  api.prepare_write().await?;
  let req = api
    .post(&format!("{V4}/answers/{}/voters", target.id()))
    .json(&json!({ "type": direction }));
  api.call(req).await?;
  Ok(Action::done(format!("vote {direction}"), target.url()))
}

/// Delete one of the account's own questions, answers, articles or pins.
pub async fn delete(api: &Api, target: &Target) -> Result<Action> {
  api.prepare_write().await?;
  let url = match target {
    Target::Question(id) => format!("{V4}/questions/{id}"),
    Target::Answer(id) => format!("{V4}/answers/{id}"),
    Target::Article(id) => format!("{ZHUANLAN}/articles/{id}"),
    Target::Pin(id) => format!("{V4}/pins/{id}"),
    Target::Zvideo(_) => return Err(Error::unsupported("delete (videos)")),
  };
  api.call(api.delete(&url)).await?;
  Ok(Action::done("delete", target.url()).with_id(target.id()))
}

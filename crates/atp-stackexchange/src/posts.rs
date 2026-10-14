//! Questions and answers: one post with its answers, the answers of a
//! question as comments, comments under a post, the question lists of
//! `hot` and `tag`.

use atp_core::text::fmt_time;
use atp_core::{Comment, Error, Page, PageReq, Post, Result, Value, ValueExt};

use crate::api::{Api, FILTER_FULL, FILTER_LIST, Params};
use crate::parse;
use crate::refs::{self, Kind, PostRef};

pub const HOT_CATEGORIES: &[&str] = &[
  "hot",
  "week",
  "month",
  "featured",
  "unanswered",
  "newest",
  "active",
];
pub const COMMENT_SORTS: &[&str] = &["votes", "newest", "oldest", "active"];

/// A question with its comments and answers written out in `text`, or an
/// answer with its comments.
pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let post = refs::post(arg)?;
  let site = api.site_or(&post.site);
  if post.kind != Kind::Answer {
    let v = api
      .get(
        &format!("questions/{}", post.id),
        Some(&site),
        Vec::new(),
        FILTER_FULL,
      )
      .await?;
    if let Some(question) = v.list("items").first() {
      return Ok(page(question));
    }
    if post.kind == Kind::Question {
      return Err(Error::not_found(format!(
        "no question {} on {site}",
        post.id
      )));
    }
  }
  let v = api
    .get(
      &format!("answers/{}", post.id),
      Some(&site),
      Vec::new(),
      FILTER_FULL,
    )
    .await?;
  let answer = v
    .list("items")
    .first()
    .ok_or_else(|| Error::not_found(format!("no question or answer {} on {site}", post.id)))?;
  let mut post = parse::answer(answer);
  post.text = Some(with_comments(&post.text.unwrap_or_default(), answer));
  Ok(post)
}

/// The question as on its page: body, comments, then every answer (highest
/// score first) with its comments.
fn page(question: &Value) -> Post {
  let mut post = parse::question(question);
  let mut text = with_comments(&post.text.unwrap_or_default(), question);
  let mut answers: Vec<&Value> = question.list("answers").iter().collect();
  answers.sort_by_key(|a| std::cmp::Reverse(a.i64("score").unwrap_or_default()));
  let total = answers.len();
  for (i, answer) in answers.iter().enumerate() {
    let comment = parse::answer_comment(answer);
    let mut head = vec![format!("Answer {} of {total}", i + 1)];
    if answer.bool("is_accepted") == Some(true) {
      head.push("accepted".into());
    }
    head.push(format!("{} votes", answer.i64("score").unwrap_or_default()));
    if let Some(author) = &comment.author {
      head.push(author.name.clone());
    }
    if let Some(at) = comment.created_at {
      head.push(fmt_time(at));
    }
    head.push(format!("id {}", comment.id));
    text.push_str(&format!("\n\n━━ {} ━━\n\n", head.join(" · ")));
    text.push_str(&with_comments(&comment.text, answer));
  }
  post.text = Some(text);
  post
}

/// A body followed by the comments under it.
fn with_comments(body: &str, v: &Value) -> String {
  let comments = v.list("comments");
  if comments.is_empty() {
    return body.to_owned();
  }
  let mut text = format!("{body}\n\n── {} comments ──", comments.len());
  for comment in comments.iter().map(parse::comment) {
    let name = comment
      .author
      .as_ref()
      .map(|a| a.name.as_str())
      .unwrap_or("?");
    let score = comment
      .likes
      .filter(|l| *l > 0)
      .map(|l| format!(" (+{l})"))
      .unwrap_or_default();
    text.push_str(&format!("\n• {name}{score}: {}", comment.text));
  }
  text
}

/// The answers of a question (with their comments as replies), or the
/// comments under an answer.
pub async fn comments(
  api: &Api,
  arg: &str,
  sort: Option<&str>,
  page: &PageReq,
) -> Result<Page<Comment>> {
  let post = refs::post(arg)?;
  let site = api.site_or(&post.site);
  if post.kind == Kind::Answer {
    return post_comments(api, &site, &post.id, page).await;
  }
  let (sort, order) = match sort.unwrap_or("votes") {
    "newest" => ("creation", "desc"),
    "oldest" => ("creation", "asc"),
    "active" => ("activity", "desc"),
    _ => ("votes", "desc"),
  };
  let params: Params = vec![("sort", sort.into()), ("order", order.into())];
  let method = format!("questions/{}/answers", post.id);
  let answers = api
    .list(
      &method,
      Some(&site),
      params,
      FILTER_FULL,
      page,
      parse::answer_comment,
    )
    .await?;
  // A bare id with no answers may be an answer itself.
  if post.kind == Kind::Any && answers.items.is_empty() && page.cursor.is_none() {
    return post_comments(api, &site, &post.id, page).await;
  }
  Ok(answers)
}

/// Comments under a question or answer, oldest first.
pub async fn replies(api: &Api, arg: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
  let post = refs::post(arg)?;
  let target = refs::post(comment)?;
  let site = api.site_or(&target.site.or(post.site));
  post_comments(api, &site, &target.id, page).await
}

async fn post_comments(api: &Api, site: &str, id: &str, page: &PageReq) -> Result<Page<Comment>> {
  let params: Params = vec![("sort", "creation".into()), ("order", "asc".into())];
  let method = format!("posts/{id}/comments");
  api
    .list(
      &method,
      Some(site),
      params,
      FILTER_FULL,
      page,
      parse::comment,
    )
    .await
}

/// `hot --category`: hot, week, month, featured, unanswered, newest, active.
pub async fn hot(api: &Api, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let (method, sort) = match category.unwrap_or("hot") {
    "featured" => ("questions/featured", "activity"),
    "unanswered" => ("questions/unanswered", "votes"),
    "newest" => ("questions", "creation"),
    "active" => ("questions", "activity"),
    other => ("questions", other),
  };
  let params: Params = vec![("sort", sort.into()), ("order", "desc".into())];
  let site = api.site();
  api
    .list(
      method,
      Some(&site),
      params,
      FILTER_LIST,
      page,
      parse::question,
    )
    .await
}

/// Questions with a tag (`;`-separated tags: all of them).
pub async fn tagged(api: &Api, tags: &str, sort: &str, page: &PageReq) -> Result<Page<Post>> {
  let sort = match sort {
    "newest" => "creation",
    "active" => "activity",
    other => other,
  };
  let params: Params = vec![
    ("tagged", tags.into()),
    ("sort", sort.into()),
    ("order", "desc".into()),
  ];
  let site = api.site();
  api
    .list(
      "questions",
      Some(&site),
      params,
      FILTER_LIST,
      page,
      parse::question,
    )
    .await
}

/// Whether a post is a question or an answer, asking the site for a bare id.
pub async fn kind(api: &Api, post: &PostRef, site: &str) -> Result<Kind> {
  if post.kind != Kind::Any {
    return Ok(post.kind);
  }
  let v = api
    .get(
      &format!("posts/{}", post.id),
      Some(site),
      Vec::new(),
      FILTER_LIST,
    )
    .await?;
  match v.str("items.0.post_type").as_deref() {
    Some("question") => Ok(Kind::Question),
    Some("answer") => Ok(Kind::Answer),
    _ => Err(Error::not_found(format!(
      "no question or answer {} on {site}",
      post.id
    ))),
  }
}

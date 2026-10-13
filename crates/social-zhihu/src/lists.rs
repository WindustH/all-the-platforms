//! More lists around users, questions, topics and columns: a user's
//! questions, videos and columns, what they follow, who follows a question
//! or topic, and the articles of a column.

use social_core::{Collection, Ctx, Page, PageReq, Post, Result, User, Value, ValueExt};

use crate::api::{self, V4};
use crate::parse;
use crate::people::{list, posts};

pub async fn questions(ctx: &Ctx, token: &str, page: &PageReq) -> Result<Page<Post>> {
  let v = list(ctx, &format!("members/{token}/questions"), None, page).await?;
  Ok(posts(&v, parse::question))
}

/// Videos of a user: video pins and `zvideo`s.
pub async fn videos(ctx: &Ctx, token: &str, page: &PageReq) -> Result<Page<Post>> {
  let v = list(ctx, &format!("members/{token}/zvideos"), None, page).await?;
  let items = v
    .list("data")
    .iter()
    .filter_map(|item| parse::post(item.at("target")).or_else(|| parse::post(item)))
    .collect();
  Ok(Page::new(items, api::next_param(&v, "offset")))
}

/// Columns a user writes in.
pub async fn columns(ctx: &Ctx, token: &str, page: &PageReq) -> Result<Page<Collection>> {
  let v = list(
    ctx,
    &format!("members/{token}/column-contributions"),
    None,
    page,
  )
  .await?;
  Ok(collections(&v, |item| parse::column(item.at("column"))))
}

/// Questions a user follows.
pub async fn following_questions(ctx: &Ctx, token: &str, page: &PageReq) -> Result<Page<Post>> {
  let v = list(
    ctx,
    &format!("members/{token}/following-questions"),
    None,
    page,
  )
  .await?;
  Ok(posts(&v, parse::question))
}

/// Topics, columns or favorites folders a user follows.
pub async fn following(
  ctx: &Ctx,
  token: &str,
  kind: &str,
  page: &PageReq,
) -> Result<Page<Collection>> {
  let (path, map): (&str, fn(&Value) -> Collection) = match kind {
    "topics" => ("following-topic-contributions", |item| {
      parse::topic(nested(item, "topic"))
    }),
    "columns" => ("following-columns", parse::column),
    _ => ("following-favlists", parse::folder),
  };
  let v = list(ctx, &format!("members/{token}/{path}"), None, page).await?;
  Ok(collections(&v, map))
}

/// Who follows a question.
pub async fn question_followers(ctx: &Ctx, id: &str, page: &PageReq) -> Result<Page<User>> {
  users(ctx, &format!("questions/{id}/followers"), page).await
}

/// Who follows a topic.
pub async fn topic_followers(ctx: &Ctx, id: &str, page: &PageReq) -> Result<Page<User>> {
  users(ctx, &format!("topics/{id}/followers"), page).await
}

pub async fn column(ctx: &Ctx, id: &str) -> Result<Collection> {
  let req = api::get(ctx, &format!("{V4}/columns/{id}")).query(
    "include",
    "intro,description,followers,articles_count,items_count",
  );
  let v = api::call(ctx, req).await?;
  Ok(parse::column(&v))
}

/// Articles of a column, newest first.
pub async fn column_items(ctx: &Ctx, id: &str, page: &PageReq) -> Result<Page<Post>> {
  let v = list(ctx, &format!("columns/{id}/items"), None, page).await?;
  let items = v.list("data").iter().filter_map(parse::post).collect();
  Ok(Page::new(items, api::next_param(&v, "offset")))
}

async fn users(ctx: &Ctx, path: &str, page: &PageReq) -> Result<Page<User>> {
  let v = list(ctx, path, None, page).await?;
  Ok(Page::new(
    v.list("data").iter().map(parse::user).collect(),
    api::next_param(&v, "offset"),
  ))
}

fn collections(v: &Value, map: fn(&Value) -> Collection) -> Page<Collection> {
  Page::new(
    v.list("data").iter().map(map).collect(),
    api::next_param(v, "offset"),
  )
}

/// `item.<key>` when the listing wraps objects (`{topic, contributions_count}`).
fn nested<'a>(item: &'a Value, key: &str) -> &'a Value {
  match item.at(key) {
    Value::Object(_) => item.at(key),
    _ => item,
  }
}

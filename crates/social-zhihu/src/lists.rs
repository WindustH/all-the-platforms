//! More lists around users, questions, topics and columns: a user's
//! questions, videos and columns, what they follow, who follows a question
//! or topic, and the articles of a column.

use social_core::{Collection, Page, PageReq, Post, Result, User, Value, ValueExt};

use crate::api::{Api, V4};
use crate::{api, parse};

pub async fn questions(api: &Api, token: &str, page: &PageReq) -> Result<Page<Post>> {
  let v = api
    .list(&format!("members/{token}/questions"), None, page)
    .await?;
  Ok(api::posts(&v, parse::question))
}

/// Videos of a user: video pins and `zvideo`s.
pub async fn videos(api: &Api, token: &str, page: &PageReq) -> Result<Page<Post>> {
  let v = api
    .list(&format!("members/{token}/zvideos"), None, page)
    .await?;
  let items = v
    .list("data")
    .iter()
    .filter_map(|item| parse::post(item.at("target")).or_else(|| parse::post(item)))
    .collect();
  Ok(Page::new(items, api::next_param(&v, "offset")))
}

/// Columns a user writes in.
pub async fn columns(api: &Api, token: &str, page: &PageReq) -> Result<Page<Collection>> {
  let v = api
    .list(&format!("members/{token}/column-contributions"), None, page)
    .await?;
  Ok(collections(&v, |item| parse::column(item.at("column"))))
}

/// Questions a user follows.
pub async fn following_questions(api: &Api, token: &str, page: &PageReq) -> Result<Page<Post>> {
  let v = api
    .list(&format!("members/{token}/following-questions"), None, page)
    .await?;
  Ok(api::posts(&v, parse::question))
}

/// Topics, columns or favorites folders a user follows.
pub async fn following(
  api: &Api,
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
  let v = api
    .list(&format!("members/{token}/{path}"), None, page)
    .await?;
  Ok(collections(&v, map))
}

/// Who follows a question.
pub async fn question_followers(api: &Api, id: &str, page: &PageReq) -> Result<Page<User>> {
  users(api, &format!("questions/{id}/followers"), page).await
}

/// Who follows a topic.
pub async fn topic_followers(api: &Api, id: &str, page: &PageReq) -> Result<Page<User>> {
  users(api, &format!("topics/{id}/followers"), page).await
}

pub async fn column(api: &Api, id: &str) -> Result<Collection> {
  let req = api.get(&format!("{V4}/columns/{id}")).query(
    "include",
    "intro,description,followers,articles_count,items_count",
  );
  let v = api.call(req).await?;
  Ok(parse::column(&v))
}

/// Articles of a column, newest first.
pub async fn column_items(api: &Api, id: &str, page: &PageReq) -> Result<Page<Post>> {
  let v = api.list(&format!("columns/{id}/items"), None, page).await?;
  let items = v.list("data").iter().filter_map(parse::post).collect();
  Ok(Page::new(items, api::next_param(&v, "offset")))
}

async fn users(api: &Api, path: &str, page: &PageReq) -> Result<Page<User>> {
  let v = api.list(path, None, page).await?;
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

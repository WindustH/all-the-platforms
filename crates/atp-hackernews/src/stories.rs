//! Stories and other items: the front-page lists of `hot` and single items.

use atp_core::{Page, PageReq, Post, Result, ValueExt};

use crate::api::{self, Api};
use crate::{parse, refs};

/// `hot --category`, each a Firebase list (`<category>stories`).
pub const HOT_CATEGORIES: &[&str] = &["top", "new", "best", "ask", "show", "job"];
const PAGE_MAX: usize = 100;

/// One page of a list of item ids (the cursor is an offset), as posts.
pub async fn hot(api: &Api, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let list = api
    .firebase(&format!("{}stories", category.unwrap_or("top")))
    .await?;
  let ids = api::ids(&list, "");
  let start = page.number_or(0) as usize;
  let size = page.size_within(PAGE_MAX);
  let slice: Vec<u64> = ids.iter().skip(start).take(size).copied().collect();
  let items = api.items(&slice).await?;
  let posts = items
    .iter()
    .filter(|item| !parse::hidden(item))
    .map(parse::item)
    .collect();
  let next = (start + size < ids.len()).then(|| (start + size).to_string());
  Ok(Page::new(posts, next))
}

/// One item; a poll lists its options with their points.
pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let v = api.item(refs::item(arg)?).await?;
  let mut post = parse::item(&v);
  let parts = api::ids(&v, "parts");
  if !parts.is_empty() {
    let options = api.items(&parts).await?;
    let lines: Vec<String> = options
      .iter()
      .map(|option| {
        let text = atp_core::text::html_to_text(&option.str("text").unwrap_or_default());
        format!(
          "• {text} ({} points)",
          option.u64("score").unwrap_or_default()
        )
      })
      .collect();
    let body = post.text.take().unwrap_or_default();
    post.text = Some(format!("{body}\n\n{}", lines.join("\n")).trim().to_owned());
  }
  Ok(post)
}

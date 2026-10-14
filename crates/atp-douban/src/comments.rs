//! Comments: a subject's short comments (短评), a review's replies and a
//! group topic's replies.

use atp_core::{Comment, Error, Page, PageReq, Result, ValueExt};

use crate::api::Api;
use crate::groups::topic_page;
use crate::parse;
use crate::refs::{self, Target};
use crate::subjects::{next_start, page_of};

/// `comments --sort` for short comments: Douban's `hot` or `latest`.
pub const SORTS: &[&str] = &["hot", "latest"];
/// The most comments one request returns.
const PAGE_MAX: usize = 50;

pub async fn list(
  api: &Api,
  arg: &str,
  sort: Option<&str>,
  page: &PageReq,
) -> Result<Page<Comment>> {
  let start = page.number_or(0);
  let mut params = vec![
    ("start", start.to_string()),
    ("count", page.size_within(PAGE_MAX).to_string()),
  ];
  let (path, referer, key) = match refs::target(arg)? {
    Target::Subject(subject) => {
      params.push(("order_by", sort.unwrap_or("hot").to_owned()));
      let path = format!("/{}/{}/interests", subject.kind, subject.id);
      (path, format!("{}comments", page_of(&subject)), "interests")
    }
    Target::Review(id) => (
      format!("/review/{id}/comments"),
      format!("/movie/review/{id}/"),
      "comments",
    ),
    Target::Topic(topic) => {
      let page = topic_page(&topic);
      params.extend(topic.spm.map(|spm| ("_spm_id", spm)));
      (
        format!("/group/topic/{}/comments", topic.id),
        page,
        "comments",
      )
    }
    Target::Group(_) => {
      return Err(Error::input(
        "a group has topics, not comments: use `atp douban group GROUP`",
      ));
    }
  };
  let v = api.get(&path, &referer, &params).await?;
  let rows = v.list(key);
  let items: Vec<Comment> = if key == "interests" {
    rows.iter().map(parse::interest).collect()
  } else {
    rows.iter().map(parse::comment).collect()
  };
  Ok(Page::new(items, next_start(&v, start, rows.len())))
}

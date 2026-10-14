//! Comments of a post and replies to a comment (neither needs a signature).
//! When `www` answers the comment list with its script challenge
//! (`__ac_nonce`), the list is read from the app host the share pages use.

use atp_core::{Comment, ErrorCode, Page, PageReq, Result, ValueExt};

use crate::api::{self, Api};
use crate::parse::comments::comment;
use crate::refs;

const PAGE: usize = 20;

pub async fn list(api: &Api, post: &str, req: &PageReq) -> Result<Page<Comment>> {
  let post = refs::post(api, post).await?;
  let offset = req.number_or(0).to_string();
  let query = [
    ("aid", "24".to_owned()),
    ("app_name", "toutiao_web".into()),
    ("offset", offset),
    ("count", req.size_within(PAGE).to_string()),
    ("group_id", post.id.clone()),
    ("item_id", post.id),
  ];
  let path = "/article/v4/tab_comments/";
  let answer = match api.get(path, &query).await {
    Err(e) if e.code == ErrorCode::VerificationRequired => {
      tracing::debug!(
        "www walled the comments ({}); asking the share host",
        e.message
      );
      api.get_at(api::SHARE, path, &query).await?
    }
    other => other?,
  };
  let v = api::check(answer)?;
  let comments: Vec<Comment> = v
    .list("data")
    .iter()
    .filter_map(|c| comment(c.at("comment")))
    .collect();
  let next = v
    .bool("has_more")
    .unwrap_or(false)
    .then(|| v.u64("offset").map(|o| o.to_string()))
    .flatten();
  Ok(Page::new(comments, next))
}

pub async fn replies(api: &Api, comment_id: &str, req: &PageReq) -> Result<Page<Comment>> {
  let offset = req.number_or(0).to_string();
  let query = [
    ("aid", "24".to_owned()),
    ("app_name", "toutiao_web".into()),
    ("id", comment_id.trim().to_owned()),
    ("offset", offset.clone()),
    ("count", req.size_within(PAGE).to_string()),
    ("repost", "0".into()),
  ];
  let v = api::check(api.get("/2/comment/v4/reply_list/", &query).await?)?;
  let data = v.at("data");
  let replies: Vec<Comment> = data.list("data").iter().filter_map(comment).collect();
  let consumed = offset.parse::<usize>().unwrap_or(0) + replies.len();
  let next = (data.bool("has_more").unwrap_or(false) && !replies.is_empty()).then(|| {
    data
      .u64("offset")
      .map_or(consumed.to_string(), |o| o.to_string())
  });
  Ok(Page::new(replies, next))
}

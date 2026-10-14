//! Comments of a post and replies to a comment, through the queries of the
//! short-video page.

use atp_core::{Comment, Page, PageReq, Result, ValueExt, json};

use crate::api::Api;
use crate::{parse, refs};

/// Verbatim from the web (the server checks the text).
const COMMENT_LIST: &str = "query commentListQuery($photoId: String, $pcursor: String) {\n  visionCommentList(photoId: $photoId, pcursor: $pcursor) {\n    commentCount\n    commentCountV2\n    pcursor\n    rootCommentsV2 {\n      commentId\n      authorId\n      authorName\n      content\n      headurl\n      timestamp\n      hasSubComments\n      likedCount\n      liked\n      status\n      __typename\n    }\n    pcursorV2\n    rootComments {\n      commentId\n      authorId\n      authorName\n      content\n      headurl\n      timestamp\n      likedCount\n      realLikedCount\n      liked\n      status\n      authorLiked\n      subCommentCount\n      subCommentsPcursor\n      subComments {\n        commentId\n        authorId\n        authorName\n        content\n        headurl\n        timestamp\n        likedCount\n        realLikedCount\n        liked\n        status\n        authorLiked\n        replyToUserName\n        replyTo\n        __typename\n      }\n      __typename\n    }\n    __typename\n  }\n}\n";

const SUB_COMMENT_LIST: &str = "mutation visionSubCommentList($photoId: String, $rootCommentId: String, $pcursor: String) {\n  visionSubCommentList(photoId: $photoId, rootCommentId: $rootCommentId, pcursor: $pcursor) {\n    pcursor\n    subComments {\n      commentId\n      authorId\n      authorName\n      content\n      headurl\n      timestamp\n      likedCount\n      realLikedCount\n      liked\n      status\n      authorLiked\n      replyToUserName\n      replyTo\n      __typename\n    }\n    pcursorV2\n    subCommentsV2 {\n      commentId\n      authorId\n      authorName\n      content\n      headurl\n      timestamp\n      hasSubComments\n      likedCount\n      liked\n      status\n      replyToUserName\n      replyTo\n      __typename\n    }\n    __typename\n  }\n}\n";

pub async fn list(api: &Api, post: &str, page: &PageReq) -> Result<Page<Comment>> {
  let id = refs::photo(api, post).await?;
  let data = api
    .graphql(
      "commentListQuery",
      json!({ "photoId": id, "pcursor": page.cursor.clone().unwrap_or_default() }),
      COMMENT_LIST,
      &refs::photo_url(&id),
    )
    .await?;
  let list = data.at("visionCommentList");
  Ok(collect(list, &["rootCommentsV2", "rootComments"], None))
}

pub async fn replies(
  api: &Api,
  post: &str,
  comment: &str,
  page: &PageReq,
) -> Result<Page<Comment>> {
  let id = refs::photo(api, post).await?;
  let data = api
    .graphql(
      "visionSubCommentList",
      json!({
        "photoId": id,
        "rootCommentId": comment,
        "pcursor": page.cursor.clone().unwrap_or_default(),
      }),
      SUB_COMMENT_LIST,
      &refs::photo_url(&id),
    )
    .await?;
  let list = data.at("visionSubCommentList");
  Ok(collect(
    list,
    &["subCommentsV2", "subComments"],
    Some(comment),
  ))
}

/// The comments of the first field that has any, and the next cursor.
fn collect(list: &atp_core::Value, fields: &[&str], parent: Option<&str>) -> Page<Comment> {
  let comments: Vec<Comment> = fields
    .iter()
    .map(|f| list.list(f))
    .find(|items| !items.is_empty())
    .unwrap_or_default()
    .iter()
    .filter_map(|c| parse::comment(c, parent))
    .collect();
  let next =
    parse::more(list.first_str(&["pcursorV2", "pcursor"])).filter(|_| !comments.is_empty());
  Page::new(comments, next)
}

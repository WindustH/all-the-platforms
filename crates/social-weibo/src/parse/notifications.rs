//! Message-box entries: comments, mentions and likes.

use super::*;

/// A message-box entry: a post or comment that mentions you, a comment you
/// received or sent, or a like.
pub fn notification(v: &Value, kind: &str) -> Option<Notification> {
  let id = v.first_str(&["idstr", "id"])?;
  let actor = user(v.at("user"));
  let (text, target, url) = match kind {
    "mention" => {
      let post = post(v)?;
      let target = post.quoted.as_ref().and_then(|q| q.text.clone());
      (
        post.text.clone().unwrap_or_default(),
        target,
        post.url.clone(),
      )
    }
    "like" => {
      let status = v.at("status");
      let target = post(status).and_then(|post| post.text);
      let url = post(status).and_then(|post| post.url);
      (
        v.str("attitude_type_text")
          .unwrap_or_else(|| "liked".into()),
        target,
        url,
      )
    }
    _ => {
      let status = v.at("status");
      let target = v
        .str("reply_comment.text_raw")
        .or_else(|| post(status).and_then(|post| post.text));
      (
        v.str("text_raw")
          .unwrap_or_else(|| strip_html(&v.str("text").unwrap_or_default())),
        target,
        post(status).and_then(|post| post.url),
      )
    }
  };
  Some(Notification {
    id,
    kind: kind.into(),
    text,
    actor,
    target: target.map(|t| social_core::text::truncate(&t, 120)),
    url,
    created_at: v.str("created_at").and_then(|t| parse_time(&t)),
    unread: None,
    raw: Some(v.clone()),
  })
}

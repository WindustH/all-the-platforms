//! Reviews (点评) as comments: of sights (`13444/json/getCommentCollapseList`,
//! with sub-scores) and of hotels (`33278/getHotelCommentList`, with the
//! hotel's answer).

use atp_core::{Comment, User, Value, ValueExt};

use super::{copy, original, put, time};
use crate::refs;

/// A review of a sight.
pub fn sight_review(v: &Value) -> Option<Comment> {
  let id = v.str("commentId")?;
  let user = v.at("userInfo");
  let client_auth = user
    .str("clientAuth")
    .filter(|_| v.bool("isAnonym") != Some(true));
  let author = User {
    url: client_auth.as_deref().map(refs::user_url),
    id: client_auth
      .or_else(|| user.str("userId"))
      .unwrap_or_default(),
    name: user.str("userNick").unwrap_or_default(),
    avatar: user.str("userImage"),
    ..User::default()
  };
  let mut comment = Comment {
    id,
    author: Some(author),
    text: v.str("content").unwrap_or_default(),
    created_at: time(v, "publishTime"),
    likes: v.count("usefulCount"),
    reply_count: v.count("replyCount"),
    location: v.str("ipLocatedName"),
    raw: Some(v.clone()),
    ..Comment::default()
  };
  let images = images(v.list("images"), &["imageSrcUrl", "imageThumbUrl"]);
  let extra = &mut comment.extra;
  put(extra, "scores", Some(scores(v.list("scores"))));
  put(extra, "images", Some(images));
  copy(extra, v, SIGHT_FIELDS);
  Some(comment)
}

/// Extra fields of a sight review: (name in `extra`, path in the review).
const SIGHT_FIELDS: &[(&str, &str)] = &[
  ("score", "score"),
  ("trip", "touristTypeDisplay"),
  ("source", "fromTypeText"),
  ("reply", "replyContent"),
  ("url", "jumpH5Url"),
];

/// `景色:5`-style sub-scores.
fn scores(list: &[Value]) -> Value {
  let list: Vec<Value> = list
    .iter()
    .filter_map(|s| Some(format!("{}:{}", s.str("name")?, s.str("score")?).into()))
    .collect();
  list.into()
}

/// Full-size pictures of a review (links, or objects with one of `keys`).
fn images(list: &[Value], keys: &[&str]) -> Value {
  let list: Vec<Value> = list
    .iter()
    .filter_map(|i| i.as_str().map(str::to_owned).or_else(|| i.first_str(keys)))
    .map(|u| original(&u).into())
    .collect();
  list.into()
}

/// A review of a hotel. Hotels rate overall only; their answer is in
/// `extra.reply`.
pub fn hotel_review(v: &Value) -> Option<Comment> {
  let id = v.str("id")?;
  let user = v.at("userInfo");
  let mut comment = Comment {
    id,
    author: Some(User {
      // Ctrip joins the letters of masked names with zero-width joiners.
      name: user
        .str("nickName")
        .unwrap_or_default()
        .replace('\u{200d}', ""),
      avatar: user.str("headPictureUrl"),
      ..User::default()
    }),
    text: v.str("content").unwrap_or_default(),
    created_at: time(v, "createDate"),
    likes: v.count("usefulCount"),
    raw: Some(v.clone()),
    ..Comment::default()
  };
  let stayed = v
    .str("checkInDate")
    .map(|d| d.chars().take(7).collect::<String>());
  let extra = &mut comment.extra;
  copy(extra, v, HOTEL_FIELDS);
  put(extra, "stayed", stayed.map(Value::from));
  put(extra, "images", Some(images(v.list("imageList"), &["url"])));
  Some(comment)
}

/// Extra fields of a hotel review: (name in `extra`, path in the review).
const HOTEL_FIELDS: &[(&str, &str)] = &[
  ("score", "ratingInfo.ratingAll"),
  ("rating", "ratingInfo.commentLevel"),
  ("room", "roomTypeName"),
  ("trip", "travelTypeText"),
  ("reply", "feedbackList.0.content"),
];

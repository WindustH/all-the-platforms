//! Comments (留言) on your articles.
//!
//! Reading: the backend (`misc/appmsgcomment?action=list_comment`, keyed by
//! the article's `comment_id`) or the API (`comment/list`, keyed by
//! `msg_data_id` = the article's `mid` and a 0-based index). Replying,
//! deleting, featuring (精选) and switching comments on or off go through
//! the API only: no public source shows the backend requests for them.
//! The API's comment endpoints are for verified accounts (仅认证).

use atp_core::text::from_secs;
use atp_core::{Action, Comment, Error, Page, PageReq, Result, User, Value, ValueExt, json};

use crate::api::Api;
use crate::backend::nested;
use crate::published;

/// The keys an article's comments hang on.
pub struct Target {
  pub mid: String,
  /// 1-based position in its group.
  pub idx: u64,
  pub comment_id: Option<String>,
}

/// From an article link (its page has all three), or an id listed by
/// `user-posts` (`BIZUIN_MID_IDX`, looked up among your recent posts).
pub async fn target(api: &Api, arg: &str) -> Result<Target> {
  if arg.contains("mp.weixin.qq.com") {
    let post = crate::articles::read(&api.ctx, arg).await?;
    let get = |k: &str| post.extra.get(k).and_then(Value::as_str).map(str::to_owned);
    let mid = get("mid").ok_or_else(|| Error::upstream("the article page names no mid"))?;
    return Ok(Target {
      mid,
      idx: get("idx").and_then(|i| i.parse().ok()).unwrap_or(1),
      comment_id: get("comment_id"),
    });
  }
  let parts: Vec<&str> = arg.split('_').collect();
  let (mid, idx) = match parts.as_slice() {
    [_, mid, idx] | [mid, idx] => (mid.to_string(), idx.parse().unwrap_or(1)),
    _ => {
      return Err(Error::input(format!(
        "not an article: {arg} (give its link, or an id from `atp mp user-posts`)"
      )));
    }
  };
  let comment_id = if api.backend().has() {
    published::find_own(api, &mid, idx).await.and_then(|p| {
      p.extra
        .get("comment_id")
        .and_then(Value::as_str)
        .map(str::to_owned)
    })
  } else {
    None
  };
  Ok(Target {
    mid,
    idx,
    comment_id,
  })
}

pub async fn list(api: &Api, arg: &str, req: &PageReq) -> Result<Page<Comment>> {
  api.require_any()?;
  let t = target(api, arg).await?;
  match (&t.comment_id, api.backend().has(), api.official().has()) {
    (Some(id), true, _) => web_list(api, id, req).await,
    (_, _, true) => api_list(api, &t, req).await,
    (None, true, false) => Err(Error::not_found(
      "the article has no comment id (comments off, or not one of your recent posts)",
    )),
    _ => Err(Error::auth("not logged in")),
  }
}

async fn web_list(api: &Api, comment_id: &str, req: &PageReq) -> Result<Page<Comment>> {
  let begin = req.number_or(0);
  let v = api
    .backend()
    .get(
      "/misc/appmsgcomment",
      vec![
        ("action", "list_comment".into()),
        ("count", "20".into()),
        ("begin", begin.to_string()),
        ("comment_id", comment_id.into()),
        ("filtertype", "0".into()),
        ("day", "0".into()),
        ("type", "2".into()),
        ("max_id", "0".into()),
        ("fingerprint", api.backend().fingerprint()),
      ],
    )
    .await?;
  let list = nested(&v, "comment_list");
  let items = list.list("comment");
  let comments: Vec<Comment> = items.iter().map(web_comment).collect();
  let next = (items.len() >= 20).then(|| (begin + 20).to_string());
  Ok(Page::new(comments, next))
}

fn web_comment(v: &Value) -> Comment {
  let mut c = Comment {
    id: v.first_str(&["content_id", "id"]).unwrap_or_default(),
    author: Some(User {
      id: v
        .str("fakeid")
        .or_else(|| v.str("openid"))
        .unwrap_or_default(),
      name: v.str("nick_name").unwrap_or_default(),
      avatar: v.str("logo_url"),
      ..User::default()
    }),
    text: v.str("content").unwrap_or_default(),
    created_at: v.i64("post_time").and_then(from_secs),
    likes: v.u64("like_num"),
    location: v
      .str("ip_wording.province_name")
      .or_else(|| v.str("ip_wording.country_name"))
      .or_else(|| v.str("ip_wording")),
    replies: v
      .list("reply.reply_list")
      .iter()
      .chain(v.list("new_reply.reply_list"))
      .map(|r| author_reply(r, "reply_id"))
      .collect(),
    raw: Some(v.clone()),
    ..Comment::default()
  };
  for (key, path) in [("elected", "is_elected"), ("top", "is_top")] {
    if let Some(b) = v.i64(path) {
      c.extra.insert(key.into(), (b == 1).into());
    }
  }
  c
}

fn author_reply(r: &Value, id_key: &str) -> Comment {
  Comment {
    id: r.str(id_key).unwrap_or_default(),
    author: Some(User {
      name: "作者".into(),
      ..User::default()
    }),
    text: r.str("content").unwrap_or_default(),
    created_at: r
      .i64("create_time")
      .or_else(|| r.i64("post_time"))
      .and_then(from_secs),
    ..Comment::default()
  }
}

async fn api_list(api: &Api, t: &Target, req: &PageReq) -> Result<Page<Comment>> {
  let begin = req.number_or(0);
  let count = req.size_within(49);
  let v = api
    .official()
    .post(
      "/cgi-bin/comment/list",
      &json!({
        "msg_data_id": t.mid.parse::<u64>().unwrap_or(0),
        "index": t.idx.saturating_sub(1),
        "begin": begin,
        "count": count,
        "type": 0,
      }),
    )
    .await?;
  let comments: Vec<Comment> = v
    .list("comment")
    .iter()
    .map(|x| {
      let mut cm = Comment {
        id: x.str("user_comment_id").unwrap_or_default(),
        author: x.str("openid").map(|id| User {
          name: id.clone(),
          id,
          ..User::default()
        }),
        text: x.str("content").unwrap_or_default(),
        created_at: x.i64("create_time").and_then(from_secs),
        replies: x
          .at("reply")
          .as_object()
          .map(|_| vec![author_reply(x.at("reply"), "")])
          .unwrap_or_default(),
        raw: Some(x.clone()),
        ..Comment::default()
      };
      cm.extra
        .insert("elected".into(), (x.i64("comment_type") == Some(1)).into());
      cm
    })
    .collect();
  let total = v.u64("total").unwrap_or(0);
  let next = (begin + (count as u64) < total).then(|| (begin + count as u64).to_string());
  Ok(Page::new(comments, next))
}

/// One of the API's comment endpoints on an article (and a comment).
async fn api_op(api: &Api, path: &str, t: &Target, extra: Value) -> Result<()> {
  let mut body = json!({
    "msg_data_id": t.mid.parse::<u64>().unwrap_or(0),
    "index": t.idx.saturating_sub(1),
  });
  if let (Some(b), Some(e)) = (body.as_object_mut(), extra.as_object()) {
    b.extend(e.clone());
  }
  api.official().post(path, &body).await?;
  Ok(())
}

fn comment_id(id: &str) -> Result<u64> {
  id.parse().map_err(|_| {
    Error::input(format!(
      "not a comment id: {id} (the `id` of `atp mp comments`)"
    ))
  })
}

/// Answer a comment (one reply per comment). Official accounts cannot post
/// comments of their own.
pub async fn reply(api: &Api, arg: &str, text: &str, to: Option<&str>) -> Result<Action> {
  let Some(to) = to else {
    return Err(Error::unsupported("comment without --reply-to").with_hint(
      "an account only answers its readers' comments: `comment ARTICLE TEXT --reply-to COMMENT`",
    ));
  };
  let t = target(api, arg).await?;
  api_op(
    api,
    "/cgi-bin/comment/reply/add",
    &t,
    json!({ "user_comment_id": comment_id(to)?, "content": text }),
  )
  .await?;
  Ok(Action::done("reply", to))
}

pub async fn delete(api: &Api, arg: &str, id: &str) -> Result<Action> {
  let t = target(api, arg).await?;
  api_op(
    api,
    "/cgi-bin/comment/delete",
    &t,
    json!({ "user_comment_id": comment_id(id)? }),
  )
  .await?;
  Ok(Action::done("delete-comment", id))
}

pub async fn delete_reply(api: &Api, arg: &str, id: &str) -> Result<Action> {
  let t = target(api, arg).await?;
  api_op(
    api,
    "/cgi-bin/comment/reply/delete",
    &t,
    json!({ "user_comment_id": comment_id(id)? }),
  )
  .await?;
  Ok(Action::done("delete-reply", id))
}

/// Feature a comment (精选), or stop featuring it.
pub async fn elect(api: &Api, arg: &str, id: &str, undo: bool) -> Result<Action> {
  let t = target(api, arg).await?;
  let path = if undo {
    "/cgi-bin/comment/unmarkelect"
  } else {
    "/cgi-bin/comment/markelect"
  };
  api_op(api, path, &t, json!({ "user_comment_id": comment_id(id)? })).await?;
  Ok(Action::done(if undo { "unelect" } else { "elect" }, id))
}

/// Open or close the comments of an article.
pub async fn switch(api: &Api, arg: &str, close: bool) -> Result<Action> {
  let t = target(api, arg).await?;
  let path = if close {
    "/cgi-bin/comment/close"
  } else {
    "/cgi-bin/comment/open"
  };
  api_op(api, path, &t, json!({})).await?;
  Ok(Action::done(
    if close {
      "close-comments"
    } else {
      "open-comments"
    },
    &t.mid,
  ))
}

/// Your articles with their newest comments (backend).
pub async fn latest(api: &Api, req: &PageReq) -> Result<Page<atp_core::Post>> {
  api.backend().require()?;
  let begin = req.number_or(0);
  let v = api
    .backend()
    .get(
      "/misc/appmsgcomment",
      vec![
        ("action", "list_latest_comment".into()),
        ("begin", begin.to_string()),
        ("count", "10".into()),
        ("sendtype", "MASSSEND".into()),
        ("scene", "1".into()),
        ("sort_type", "1".into()),
      ],
    )
    .await?;
  let list = nested(&v, "app_msg_list");
  let items = list.list("app_msg");
  let posts: Vec<atp_core::Post> = items
    .iter()
    .map(|a| {
      let mut p = atp_core::Post {
        id: a.str("id").unwrap_or_default(),
        kind: "article".into(),
        title: a.str("item.title").or_else(|| a.str("comment.title")),
        raw: Some(a.clone()),
        ..atp_core::Post::default()
      };
      p.metrics.comments = a.u64("comment.total_count");
      if let Some(id) = a.str("item.comment_id") {
        p.extra.insert("comment_id".into(), id.into());
      }
      let preview: Vec<Value> = a
        .list("comment.comment")
        .iter()
        .filter_map(|x| x.str("content").map(Value::from))
        .collect();
      if !preview.is_empty() {
        p.extra.insert("latest".into(), Value::from(preview));
      }
      p
    })
    .collect();
  let next = (items.len() >= 10).then(|| (begin + 10).to_string());
  Ok(Page::new(posts, next))
}

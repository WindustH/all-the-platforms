//! Reading posts: one post, your threads and replies, another account's
//! threads, the conversation under a post, and the replies to a reply.

use atp_core::{Comment, Error, Page, PageReq, Post, Result, ValueExt};
use atp_meta::graph::{Params, fields, params};

use crate::api::Api;
use crate::parse::{self, POST_FIELDS, REPLY_FIELDS};
use crate::refs::{self, PostRef, UserRef};

/// The most rows one request lists.
const PAGE_MAX: usize = 100;
/// How far back a link's shortcode is looked for among an account's posts.
const SCAN_MAX: usize = 500;

/// A post argument → its id. Links are matched to ids among the posts ATP
/// listed before, else among the author's latest posts.
pub async fn id(api: &Api, arg: &str) -> Result<String> {
  match refs::post(arg)? {
    PostRef::Id(id) => Ok(id),
    PostRef::Code { user, code } => match api.known(&code) {
      Some(id) => Ok(id),
      None => find(api, user.as_deref(), &code).await,
    },
  }
}

async fn find(api: &Api, user: Option<&str>, code: &str) -> Result<String> {
  let me = api.username().await?;
  let graph = api.graph().await?;
  let wanted = fields("id,shortcode");
  let lists: Vec<(&str, Params)> = match user {
    Some(user) if user != me => {
      let mut query = params(&[("username", user)]);
      query.extend(wanted);
      vec![("profile_posts", query)]
    }
    _ => vec![("me/threads", wanted.clone()), ("me/replies", wanted)],
  };
  for (path, query) in lists {
    let items = graph.all(path, &query, SCAN_MAX).await?;
    let found = items
      .iter()
      .find(|item| item.str("shortcode").as_deref() == Some(code));
    if let Some(id) = found.and_then(|item| item.str("id")) {
      return Ok(id);
    }
  }
  Err(
    Error::not_found(format!(
      "post {code} is not among the author's latest {SCAN_MAX} posts"
    ))
    .with_hint(
      "pass the post's id, or list it first (`user-posts`, `search`) so its link is known",
    ),
  )
}

pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let id = id(api, arg).await?;
  let v = api.graph().await?.get(&id, &fields(POST_FIELDS)).await?;
  let post = parse::post(&v);
  api.remember(std::slice::from_ref(&post));
  Ok(post)
}

/// One page of posts from `path`.
pub async fn list(api: &Api, path: &str, mut query: Params, req: &PageReq) -> Result<Page<Post>> {
  query.extend(fields(POST_FIELDS));
  let (rows, next) = api.graph().await?.page(path, &query, req, PAGE_MAX).await?;
  let posts: Vec<Post> = rows.iter().map(parse::post).collect();
  api.remember(&posts);
  Ok(Page::new(posts, next))
}

/// Your threads, or another account's (`profile_posts`, which needs the
/// `threads_profile_discovery` permission).
pub async fn user_posts(api: &Api, arg: &str, req: &PageReq) -> Result<Page<Post>> {
  match refs::user(arg)? {
    UserRef::Me => list(api, "me/threads", Vec::new(), req).await,
    UserRef::Name(name) if name == api.username().await? => {
      list(api, "me/threads", Vec::new(), req).await
    }
    UserRef::Name(name) => {
      let query = params(&[("username", &name)]);
      list(api, "profile_posts", query, req)
        .await
        .map_err(discovery_hint)
    }
  }
}

/// Another account's data needs the profile discovery permission.
pub fn discovery_hint(e: Error) -> Error {
  if e.code != atp_core::ErrorCode::PermissionDenied {
    return e;
  }
  e.with_hint(
    "other accounts need the threads_profile_discovery permission on your app (and Meta's \
     approval for accounts that are not the app's testers)",
  )
}

/// The conversation under a post (every reply, at any depth), or the
/// direct replies to a reply (`direct`).
pub async fn replies(api: &Api, arg: &str, direct: bool, req: &PageReq) -> Result<Page<Comment>> {
  let id = id(api, arg).await?;
  let edge = if direct { "replies" } else { "conversation" };
  let mut query = fields(REPLY_FIELDS);
  query.extend(params(&[("reverse", "false")]));
  let (rows, next) = api
    .graph()
    .await?
    .page(&format!("{id}/{edge}"), &query, req, PAGE_MAX)
    .await?;
  let posts: Vec<Post> = rows.iter().map(parse::post).collect();
  api.remember(&posts);
  Ok(Page::new(rows.iter().map(parse::comment).collect(), next))
}

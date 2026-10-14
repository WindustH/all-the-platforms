//! Reading posts: the Discover feed and custom feeds, your timeline, one
//! post and its thread, likes, quotes and reposts of it, your bookmarks
//! and likes, and the media of a post.

use atp_core::paging::slice;
use atp_core::{
  Comment, Error, Media, MediaKind, Page, PageReq, Post, Result, User, Value, ValueExt,
};

use crate::api::{Api, HANDLE_KEY};
use crate::refs::{self, ActorRef};
use crate::{identity, parse};

pub const FEED_KINDS: &[&str] = &["following", "discover"];
/// Bluesky's Discover feed.
pub const DISCOVER: &str =
  "at://did:plc:z72i7hdynmk6r22z27h6tvur/app.bsky.feed.generator/whats-hot";
/// How deep `comments` follows a thread.
const THREAD_DEPTH: &str = "10";

/// A post argument → its `at://` URI with the author's DID.
pub async fn uri(api: &Api, arg: &str) -> Result<String> {
  let mut at = refs::post(arg)?;
  at.repo = identity::did(api, &at.repo).await?;
  Ok(at.uri())
}

/// One post view by URI (`getPosts`).
pub async fn view(api: &Api, uri: &str) -> Result<Value> {
  let v = api
    .get("app.bsky.feed.getPosts", vec![("uris", uri.to_owned())])
    .await?;
  let post = v.at("posts.0");
  if !post.is_object() {
    return Err(Error::not_found(format!(
      "no post {uri} (deleted, or hidden from you)"
    )));
  }
  Ok(post.clone())
}

pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let uri = uri(api, arg).await?;
  Ok(parse::post(&view(api, &uri).await?))
}

/// The posts of a custom feed (a feed generator).
pub async fn generator(api: &Api, feed: &str, page: &PageReq) -> Result<Page<Post>> {
  let mut at = refs::feed(feed)?;
  at.repo = identity::did(api, &at.repo).await?;
  let query = vec![("feed", at.uri())];
  api
    .list(
      "app.bsky.feed.getFeed",
      query,
      "feed",
      page,
      parse::feed_item,
    )
    .await
}

/// `feed`: your timeline (`following`, the default when logged in) or
/// Discover.
pub async fn timeline(api: &Api, kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let kind = kind.unwrap_or(if api.logged_in() {
    "following"
  } else {
    "discover"
  });
  match kind {
    "following" => {
      api.did()?;
      api
        .list(
          "app.bsky.feed.getTimeline",
          Vec::new(),
          "feed",
          page,
          parse::feed_item,
        )
        .await
    }
    "discover" => generator(api, DISCOVER, page).await,
    other => Err(Error::input(format!("unknown feed: {other}"))),
  }
}

/// The replies of a post, as threads (`getPostThread`).
pub async fn thread(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Comment>> {
  let uri = uri(api, arg).await?;
  let query = vec![
    ("uri", uri.clone()),
    ("depth", THREAD_DEPTH.to_owned()),
    ("parentHeight", "0".to_owned()),
  ];
  let v = api.get("app.bsky.feed.getPostThread", query).await?;
  let thread = v.at("thread");
  if !thread.at("post").is_object() {
    return Err(Error::not_found(format!(
      "no thread for {uri} (deleted, or hidden from you)"
    )));
  }
  let replies: Vec<Comment> = thread
    .list("replies")
    .iter()
    .filter_map(parse::comment)
    .collect();
  Ok(slice(replies, page))
}

pub async fn likers(api: &Api, arg: &str, page: &PageReq) -> Result<Page<User>> {
  let query = vec![("uri", uri(api, arg).await?)];
  api
    .list("app.bsky.feed.getLikes", query, "likes", page, |like| {
      Some(parse::user(like.at("actor")))
    })
    .await
}

/// Posts that quote a post.
pub async fn quotes(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let query = vec![("uri", uri(api, arg).await?)];
  api
    .list("app.bsky.feed.getQuotes", query, "posts", page, |v| {
      Some(parse::post(v))
    })
    .await
}

/// Who reposted a post.
pub async fn reposted_by(api: &Api, arg: &str, page: &PageReq) -> Result<Page<User>> {
  let query = vec![("uri", uri(api, arg).await?)];
  api
    .list(
      "app.bsky.feed.getRepostedBy",
      query,
      "repostedBy",
      page,
      |v| Some(parse::user(v)),
    )
    .await
}

/// Fail unless `user` is unset or you.
fn own_only(api: &Api, user: Option<&str>, what: &str) -> Result<()> {
  let did = api.did()?;
  let handle = api.ctx.extra(HANDLE_KEY).unwrap_or_default();
  match user.map(refs::actor).transpose()? {
    None | Some(ActorRef::Me) => Ok(()),
    Some(ActorRef::Actor(actor)) if actor == did || actor == handle => Ok(()),
    Some(_) => Err(
      Error::unsupported(format!("{what} of another account"))
        .with_hint("Bluesky shows only your own"),
    ),
  }
}

/// Your bookmarks (`app.bsky.bookmark.getBookmarks`).
pub async fn bookmarks(api: &Api, user: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  own_only(api, user, "bookmarks")?;
  api
    .list(
      "app.bsky.bookmark.getBookmarks",
      Vec::new(),
      "bookmarks",
      page,
      |b| {
        let item = b.at("item");
        item.at("uri").is_string().then(|| parse::post(item))
      },
    )
    .await
}

/// The posts you liked (`getActorLikes`, yours only).
pub async fn likes(api: &Api, user: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  own_only(api, user, "likes")?;
  let query = vec![("actor", api.did()?)];
  api
    .list(
      "app.bsky.feed.getActorLikes",
      query,
      "feed",
      page,
      parse::feed_item,
    )
    .await
}

/// The post and its media; a video is fetched whole from the author's PDS
/// (`com.atproto.sync.getBlob`) instead of its streaming playlist.
pub async fn media(api: &Api, arg: &str) -> Result<(Post, Vec<Media>)> {
  let post = read(api, arg).await?;
  let mut media = post.media.clone();
  let cid = post.extra.get("video_cid").and_then(Value::as_str);
  let did = post.author.as_ref().map(|a| a.id.clone());
  if let (Some(cid), Some(did)) = (cid, did) {
    let pds = identity::pds(api, &did).await?;
    let blob = format!("{pds}/xrpc/com.atproto.sync.getBlob?did={did}&cid={cid}");
    for item in media.iter_mut().filter(|m| m.kind == MediaKind::Video) {
      item.url = blob.clone();
    }
  }
  Ok((post, media))
}

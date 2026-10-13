//! `listen`: polls the inbox, subreddits (`r/name`: new posts and comments)
//! or users (`u/name`) about every 30 s and prints what is new. The first
//! poll only notes what is already there.

use std::time::Duration;

use atp_core::live;
use atp_core::output::Stream;
use atp_core::{Comment, Error, Notification, Post, Result, Value};

use crate::api::Api;
use crate::refs::{Target, target};
use crate::{listings, parse};

/// How often the sources are read.
const POLL: Duration = Duration::from_secs(30);

enum Source {
  Inbox,
  Sub(String),
  User(String),
}

impl Source {
  fn label(&self) -> String {
    match self {
      Source::Inbox => "inbox".into(),
      Source::Sub(n) => format!("r/{n}"),
      Source::User(n) => format!("u/{n}"),
    }
  }
}

fn sources(api: &Api, chats: &[String]) -> Result<Vec<Source>> {
  if chats.is_empty() {
    if !api.logged_in() {
      return Err(Error::input(
        "name what to follow (r/SUB, u/USER), or log in to follow your inbox",
      ));
    }
    return Ok(vec![Source::Inbox]);
  }
  chats
    .iter()
    .map(|c| {
      if c.eq_ignore_ascii_case("inbox") {
        api.require_login()?;
        return Ok(Source::Inbox);
      }
      Ok(match target(c)? {
        Target::Subreddit(n) => Source::Sub(n),
        Target::User(n) => Source::User(n),
      })
    })
    .collect()
}

fn from_comment(c: Comment) -> Post {
  let mut post = Post {
    id: format!("t1_{}", c.id),
    kind: "comment".into(),
    text: Some(c.text),
    url: c
      .extra
      .get("url")
      .and_then(Value::as_str)
      .map(str::to_owned),
    author: c.author,
    created_at: c.created_at,
    raw: c.raw,
    ..Post::default()
  };
  post.extra = c.extra;
  post
}

fn from_notification(n: Notification) -> Post {
  Post {
    id: n.id,
    kind: n.kind,
    title: n.target,
    text: Some(n.text),
    url: n.url,
    author: n.actor,
    created_at: n.created_at,
    raw: n.raw,
    ..Post::default()
  }
}

/// The newest items of a source.
async fn latest(api: &Api, source: &Source) -> Result<Vec<Post>> {
  let limit = || ("limit", "25".to_owned());
  let comments = |v: &Value| -> Vec<Post> {
    listings::of(v, |t| parse::comment(t, None))
      .items
      .into_iter()
      .map(from_comment)
      .collect()
  };
  Ok(match source {
    Source::Inbox => {
      let v = api
        .get("/message/inbox", &[("mark", "false".into()), limit()])
        .await?;
      listings::of(&v, parse::notification)
        .items
        .into_iter()
        .map(from_notification)
        .collect()
    }
    Source::Sub(n) => {
      let posts = api.get(&format!("/r/{n}/new"), &[limit()]).await?;
      let mut out = listings::of(&posts, listings::post).items;
      let v = api.get(&format!("/r/{n}/comments"), &[limit()]).await?;
      out.extend(comments(&v));
      out
    }
    Source::User(n) => {
      let sort = ("sort", "new".to_owned());
      let posts = api
        .get(&format!("/user/{n}/submitted"), &[sort.clone(), limit()])
        .await?;
      let mut out = listings::of(&posts, listings::post).items;
      let v = api
        .get(&format!("/user/{n}/comments"), &[sort, limit()])
        .await?;
      out.extend(comments(&v));
      out
    }
  })
}

pub async fn listen(api: &Api, chats: &[String], out: &Stream) -> Result<()> {
  let sources = sources(api, chats)?;
  live::poll(out, POLL, async || {
    let mut items = Vec::new();
    for source in &sources {
      for mut post in latest(api, source).await? {
        post.extra.insert("chat_name".into(), source.label().into());
        items.push(post);
      }
    }
    Ok(items)
  })
  .await
}

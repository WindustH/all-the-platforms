//! Statuses: timelines, trends, one status and its thread, who liked it,
//! your likes and bookmarks, and every change to a status.

use std::path::PathBuf;

use atp_core::http::Method;
use atp_core::paging::slice;
use atp_core::{
  Action, Comment, Draft, Error, Page, PageReq, Post, Reply, Result, User, Value, ValueExt,
};

use crate::api::{Api, Query};
use crate::parse;
use crate::refs::{self, StatusRef};
use crate::users;

pub const FEED_KINDS: &[&str] = &["home", "local", "public", "remote"];
pub const HOT_CATEGORIES: &[&str] = &["statuses", "links", "local", "public"];

/// A status argument → its id on this instance (links to other servers
/// are looked up with `resolve`, which needs an account).
pub async fn status_id(api: &Api, arg: &str) -> Result<String> {
  match refs::status(arg, &api.instance())? {
    StatusRef::Id(id) => Ok(id),
    StatusRef::Remote(url) => {
      api.require()?;
      let query = vec![
        ("q", url.clone()),
        ("type", "statuses".to_owned()),
        ("resolve", "true".to_owned()),
      ];
      let v = api.get("v2/search", query).await?;
      v.str("statuses.0.id")
        .ok_or_else(|| Error::not_found(format!("{url}: the instance found no status there")))
    }
  }
}

/// `feed`: home (logged in) or the local, public or remote timeline.
pub async fn timeline(api: &Api, kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let kind = kind.unwrap_or(if api.logged_in() { "home" } else { "local" });
  let (path, query): (&str, Query) = match kind {
    "home" => {
      api.require()?;
      ("v1/timelines/home", Vec::new())
    }
    "local" => ("v1/timelines/public", vec![("local", "true".into())]),
    "public" => ("v1/timelines/public", Vec::new()),
    "remote" => ("v1/timelines/public", vec![("remote", "true".into())]),
    other => return Err(Error::input(format!("unknown timeline: {other}"))),
  };
  api.list(path, query, page, parse::post).await
}

/// `hot`: trending statuses or links, or the local / public timeline.
pub async fn hot(api: &Api, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  match category.unwrap_or("statuses") {
    "statuses" => {
      api
        .list("v1/trends/statuses", Vec::new(), page, parse::post)
        .await
    }
    "links" => {
      api
        .list("v1/trends/links", Vec::new(), page, parse::link)
        .await
    }
    timeline => self::timeline(api, Some(timeline), page).await,
  }
}

pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let id = status_id(api, arg).await?;
  let v = api.get(&format!("v1/statuses/{id}"), Vec::new()).await?;
  Ok(parse::post(&v, &api.instance()))
}

/// The statuses after one in its thread (`context.descendants`), all at
/// once; `direct` keeps only the answers to it.
pub async fn descendants(
  api: &Api,
  arg: &str,
  direct: bool,
  page: &PageReq,
) -> Result<Page<Comment>> {
  let id = status_id(api, arg).await?;
  let v = api
    .get(&format!("v1/statuses/{id}/context"), Vec::new())
    .await?;
  let instance = api.instance();
  let comments: Vec<Comment> = v
    .list("descendants")
    .iter()
    .filter(|s| !direct || s.str("in_reply_to_id").as_deref() == Some(id.as_str()))
    .map(|s| parse::comment(s, &instance))
    .collect();
  Ok(slice(comments, page))
}

/// Who liked a status.
pub async fn likers(api: &Api, arg: &str, page: &PageReq) -> Result<Page<User>> {
  let id = status_id(api, arg).await?;
  let path = format!("v1/statuses/{id}/favourited_by");
  api.list(&path, Vec::new(), page, parse::user).await
}

/// Your bookmarks (`favorites`) or likes (`likes`); Mastodon shows nobody
/// else's.
pub async fn saved(
  api: &Api,
  user: Option<&str>,
  bookmarks: bool,
  page: &PageReq,
) -> Result<Page<Post>> {
  if user.is_some_and(|u| u != "me") {
    return Err(
      Error::unsupported("another account's likes or bookmarks")
        .with_hint("Mastodon shows only your own"),
    );
  }
  api.require()?;
  let path = if bookmarks {
    "v1/bookmarks"
  } else {
    "v1/favourites"
  };
  api.list(path, Vec::new(), page, parse::post).await
}

// ── writing ─────────────────────────────────────────────────────────────

/// What `POST /api/v1/statuses` takes besides the text.
#[derive(Default)]
pub struct StatusOptions {
  pub cw: Option<String>,
  pub visibility: Option<String>,
  pub language: Option<String>,
  pub sensitive: bool,
  pub reply_to: Option<String>,
  pub quote: Option<String>,
  pub files: Vec<PathBuf>,
}

pub async fn create(api: &Api, text: &str, options: &StatusOptions) -> Result<Action> {
  api.require()?;
  let mut form: Query = Vec::new();
  if !text.is_empty() {
    form.push(("status", text.to_owned()));
  }
  for (key, value) in [
    ("spoiler_text", &options.cw),
    ("visibility", &options.visibility),
    ("language", &options.language),
  ] {
    if let Some(value) = value {
      form.push((key, value.clone()));
    }
  }
  if options.sensitive {
    form.push(("sensitive", "true".into()));
  }
  let action = if let Some(reply_to) = &options.reply_to {
    form.push(("in_reply_to_id", status_id(api, reply_to).await?));
    "reply"
  } else {
    "publish"
  };
  if let Some(quote) = &options.quote {
    form.push(("quoted_status_id", status_id(api, quote).await?));
  }
  for file in &options.files {
    form.push(("media_ids[]", api.upload(file, None).await?));
  }
  let v = api.write(Method::POST, "v1/statuses", form).await?;
  let post = parse::post(&v, &api.instance());
  let mut done = Action::done(action, &post.id).with_id(&post.id);
  if let Some(url) = post.url {
    done = done.with_url(url);
  }
  Ok(done)
}

/// `post`: the title becomes the content warning, topics hashtags.
pub async fn publish(api: &Api, draft: &Draft) -> Result<Action> {
  let mut text = draft.text.clone();
  for topic in &draft.topics {
    text.push_str(&format!(" #{}", topic.trim_start_matches('#')));
  }
  let options = StatusOptions {
    cw: draft.title.clone(),
    reply_to: draft.reply_to.clone(),
    quote: draft.quote.clone(),
    files: draft.images.clone(),
    ..StatusOptions::default()
  };
  create(api, text.trim(), &options).await
}

/// A reply to `post` (or to the reply it answers), seen by the same
/// people as the status it answers.
pub async fn comment(api: &Api, post: &str, reply: &Reply) -> Result<Action> {
  let target = reply.reply_to.clone().unwrap_or_else(|| post.to_owned());
  let id = status_id(api, &target).await?;
  let parent = api.get(&format!("v1/statuses/{id}"), Vec::new()).await?;
  let options = StatusOptions {
    reply_to: Some(id),
    visibility: parent.str("visibility"),
    cw: parent.str("spoiler_text").filter(|cw| !cw.is_empty()),
    files: reply.images.clone(),
    ..StatusOptions::default()
  };
  let me = users::my_id(api).await?;
  let text = if parent.str("account.id") == Some(me) {
    reply.text.trim().to_owned()
  } else {
    mention(&parent, &api.instance(), reply.text.trim())
  };
  create(api, &text, &options).await
}

/// The text with the author of `parent` mentioned first, as Mastodon's
/// own clients write replies (only mentioned accounts hear of a reply).
fn mention(parent: &Value, instance: &str, text: &str) -> String {
  let Some(acct) = parent.str("account.acct") else {
    return text.to_owned();
  };
  let full = if acct.contains('@') {
    acct
  } else {
    format!("{acct}@{instance}")
  };
  let handle = format!("@{full}");
  let short = format!("@{}", full.split('@').next().unwrap_or_default());
  if text.contains(&handle) || text.split_whitespace().any(|w| w == short) {
    return text.to_owned();
  }
  format!("{handle} {text}")
}

/// One of the `POST /api/v1/statuses/:id/<verb>` toggles.
pub async fn toggle(
  api: &Api,
  arg: &str,
  verb: &str,
  undo_verb: &str,
  undo: bool,
) -> Result<Action> {
  let id = status_id(api, arg).await?;
  let verb = if undo { undo_verb } else { verb };
  let v = api
    .write(
      Method::POST,
      &format!("v1/statuses/{id}/{verb}"),
      Vec::new(),
    )
    .await?;
  let mut done = Action::done(verb, &id);
  if let Some(url) = parse::post(&v, &api.instance()).url {
    done = done.with_url(url);
  }
  Ok(done)
}

pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  let id = status_id(api, arg).await?;
  api
    .write(Method::DELETE, &format!("v1/statuses/{id}"), Vec::new())
    .await?;
  Ok(Action::done("delete", &id))
}

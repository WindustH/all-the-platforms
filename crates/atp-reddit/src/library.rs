//! Your own lists beyond saved and upvoted posts (saved comments, hidden and
//! downvoted posts), hiding posts, trophies, moderated subreddits, and
//! discovery: search inside one subreddit and posts linking a domain.

use atp_core::text::from_secs;
use atp_core::{
  Action, Collection, Comment, Error, Page, PageReq, Post, Result, Value, ValueExt, json,
};

use crate::api::{Api, WWW};
use crate::refs::{self, sub_url};
use crate::{listings, parse};

/// Comments you saved.
pub async fn saved_comments(api: &Api, req: &PageReq) -> Result<Page<Comment>> {
  api.require_login()?;
  let name = api.username().await?;
  let query = vec![("type", "comments".into())];
  listings::page(api, &format!("/user/{name}/saved"), query, req, |t| {
    parse::comment(t, None)
  })
  .await
}

/// Hide a post from your listings, or show it again with `undo`.
pub async fn hide(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let id = refs::post(&api.ctx, arg).await?;
  let (path, name) = if undo {
    ("/api/unhide", "unhide")
  } else {
    ("/api/hide", "hide")
  };
  api.post(path, vec![("id", format!("t3_{id}"))]).await?;
  Ok(Action::done(name, format!("t3_{id}")).with_url(format!("{WWW}/comments/{id}/")))
}

/// A user's trophies (public).
pub async fn trophies(api: &Api, arg: &str) -> Result<Value> {
  let name = refs::user(arg)?;
  let v = api
    .get_oauth(&format!("/api/v1/user/{name}/trophies"), &[])
    .await?;
  let trophies: Vec<Value> = v
    .list("data.trophies")
    .iter()
    .map(|t| {
      let d = t.at("data");
      json!({
        "name": d.str("name"),
        "description": d.str("description"),
        "granted_at": d.i64("granted_at").and_then(from_secs).map(|t| t.to_string()),
        "icon": d.str("icon_70"),
        "url": d.str("url").map(|u| if u.starts_with('/') { format!("{WWW}{u}") } else { u }),
      })
    })
    .collect();
  Ok(json!({ "user": name, "trophies": trophies }))
}

/// Subreddits a user moderates.
pub async fn moderated(api: &Api, user: Option<&str>) -> Result<Page<Collection>> {
  let name = match user {
    Some(u) => refs::user(u)?,
    None => api.username().await?,
  };
  let v = api
    .get(&format!("/user/{name}/moderated_subreddits"), &[])
    .await?;
  let subs = v
    .list("data")
    .iter()
    .filter_map(|s| {
      let sr = s.str("sr")?;
      let mut c = Collection {
        id: s.str("name").unwrap_or_else(|| sr.clone()),
        kind: "subreddit".into(),
        name: sr.clone(),
        description: s.str("title"),
        url: Some(sub_url(&sr)),
        followers: s.u64("subscribers"),
        ..Collection::default()
      };
      if s.bool("over_18") == Some(true) {
        c.extra.insert("nsfw".into(), true.into());
      }
      Some(c)
    })
    .collect();
  Ok(Page::last(subs))
}

/// Posts of one subreddit matching `q` (Reddit's search syntax).
pub async fn search_in(
  api: &Api,
  sub: &str,
  q: &str,
  sort: &str,
  time: Option<&str>,
  req: &PageReq,
) -> Result<Page<Post>> {
  let name = refs::subreddit(sub)?;
  let mut query = vec![
    ("q", q.to_owned()),
    ("restrict_sr", "1".into()),
    ("type", "link".into()),
    ("sort", sort.to_owned()),
  ];
  if let Some(t) = time {
    query.push(("t", t.to_owned()));
  }
  listings::posts(api, &format!("/r/{name}/search"), query, req).await
}

/// Posts linking to a domain (`github.com`).
pub async fn domain(
  api: &Api,
  domain: &str,
  sort: &str,
  time: Option<&str>,
  req: &PageReq,
) -> Result<Page<Post>> {
  let host = domain
    .trim()
    .trim_start_matches("https://")
    .trim_start_matches("http://")
    .trim_end_matches('/');
  if host.is_empty() || host.contains('/') {
    return Err(Error::input(format!("not a domain: {domain}")));
  }
  let query = time.map(|t| ("t", t.to_owned())).into_iter().collect();
  listings::posts(api, &format!("/domain/{host}/{sort}"), query, req).await
}

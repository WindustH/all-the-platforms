//! Subreddits: search, your subscriptions, community listings and details,
//! (un)subscribing (also how Reddit follows a user's profile), and a
//! subreddit's rules, moderators, wiki and post flairs.

use social_core::text::{from_secs, html_to_text};
use social_core::{
  Action, Collection, Error, Page, PageReq, Post, Query, Result, User, Value, ValueExt, json,
};

use crate::api::{Api, WWW};
use crate::refs::{self, Target, sub_url, user_url};
use crate::{listings, parse};

async fn communities(
  api: &Api,
  path: &str,
  query: Vec<(&str, String)>,
  req: &PageReq,
) -> Result<Page<Collection>> {
  listings::page(api, path, query, req, |t| {
    parse::data(t, "t5").and_then(parse::subreddit)
  })
  .await
}

pub async fn search(api: &Api, q: &Query, req: &PageReq) -> Result<Page<Collection>> {
  communities(
    api,
    "/subreddits/search",
    vec![("q", q.keyword.clone())],
    req,
  )
  .await
}

/// `popular`, `new` or `default` communities.
pub async fn listed(api: &Api, which: &str, req: &PageReq) -> Result<Page<Collection>> {
  communities(api, &format!("/subreddits/{which}"), Vec::new(), req).await
}

/// Communities (and followed profiles) of the logged-in account.
pub async fn subscriptions(
  api: &Api,
  user: Option<&str>,
  req: &PageReq,
) -> Result<Page<Collection>> {
  api.require_login()?;
  if let Some(user) = user
    && !refs::user(user)?.eq_ignore_ascii_case(&api.username().await?)
  {
    return Err(Error::input(
      "Reddit lists the subscriptions of the logged-in account only",
    ));
  }
  communities(api, "/subreddits/mine/subscriber", Vec::new(), req).await
}

pub async fn about(api: &Api, arg: &str) -> Result<Collection> {
  let name = refs::subreddit(arg)?;
  let v = api.get(&format!("/r/{name}/about"), &[]).await?;
  parse::data(&v, "t5")
    .and_then(parse::subreddit)
    .ok_or_else(|| Error::not_found(format!("no such subreddit: r/{name}")))
}

/// Subscribe to `r/name`, or follow `u/name` (its profile subreddit `u_name`).
pub async fn subscribe(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let (sr_name, url) = match refs::target(arg)? {
    Target::Subreddit(name) => (name.clone(), sub_url(&name)),
    Target::User(name) => (format!("u_{name}"), user_url(&name)),
  };
  let mut form = vec![
    ("action", if undo { "unsub" } else { "sub" }.into()),
    ("sr_name", sr_name.clone()),
  ];
  if !undo {
    form.push(("skip_initial_defaults", "true".into()));
  }
  api.post("/api/subscribe", form).await?;
  let name = if undo { "unfollow" } else { "follow" };
  Ok(Action::done(name, sr_name).with_url(url))
}

/// Subreddit rules in order, then Reddit's site-wide reasons.
pub async fn rules(api: &Api, arg: &str) -> Result<Value> {
  let name = refs::subreddit(arg)?;
  let v = api.get(&format!("/r/{name}/about/rules"), &[]).await?;
  let rules: Vec<Value> = v
    .list("rules")
    .iter()
    .map(|r| {
      json!({
        "rule": r.str("short_name"),
        "description": r.str("description"),
        "applies_to": r.str("kind"),
        "report_reason": r.str("violation_reason"),
        "created_at": r.i64("created_utc").and_then(from_secs).map(|t| t.to_string()),
      })
    })
    .collect();
  Ok(json!({ "subreddit": name, "rules": rules, "site_rules": v.at("site_rules") }))
}

/// Moderators with their permissions and since when.
pub async fn moderators(api: &Api, arg: &str) -> Result<Page<User>> {
  let name = refs::subreddit(arg)?;
  let v = api.get(&format!("/r/{name}/about/moderators"), &[]).await?;
  let users = v
    .list("data.children")
    .iter()
    .filter_map(|m| {
      let user = m.str("name")?;
      let mut u = parse::named(&user, m.str("id"));
      u.extra
        .insert("permissions".into(), m.at("mod_permissions").clone());
      if let Some(t) = m.i64("date").and_then(from_secs) {
        u.extra.insert("since".into(), t.to_string().into());
      }
      Some(u)
    })
    .collect();
  Ok(Page::last(users))
}

/// The names of a subreddit's wiki pages.
pub async fn wiki_pages(api: &Api, arg: &str) -> Result<Page<Collection>> {
  let name = refs::subreddit(arg)?;
  let v = api.get(&format!("/r/{name}/wiki/pages"), &[]).await?;
  let pages = v
    .list("data")
    .iter()
    .filter_map(|p| p.as_str())
    .map(|page| Collection {
      id: format!("{name}/{page}"),
      kind: "wiki".into(),
      name: page.to_owned(),
      url: Some(format!("{WWW}/r/{name}/wiki/{page}")),
      ..Collection::default()
    })
    .collect();
  Ok(Page::last(pages))
}

/// One wiki page (`index` by default) as a post: its Markdown, last revision
/// and editor.
pub async fn wiki_page(api: &Api, arg: &str, page: &str) -> Result<Post> {
  let name = refs::subreddit(arg)?;
  let v = api.get(&format!("/r/{name}/wiki/{page}"), &[]).await?;
  let d = v.at("data");
  let text = d
    .str("content_md")
    .or_else(|| d.str("content_html").map(|h| html_to_text(&h)))
    .ok_or_else(|| Error::not_found(format!("no wiki page r/{name}/wiki/{page}")))?;
  let editor = d
    .str("revision_by.data.name")
    .map(|n| parse::named(&n, d.str("revision_by.data.id")));
  let mut post = Post {
    id: format!("{name}/{page}"),
    kind: "wiki".into(),
    title: Some(page.to_owned()),
    text: Some(text),
    url: Some(format!("{WWW}/r/{name}/wiki/{page}")),
    author: editor,
    updated_at: d.i64("revision_date").and_then(from_secs),
    raw: Some(d.clone()),
    ..Post::default()
  };
  post.extra.insert("subreddit".into(), name.into());
  if let Some(reason) = d.str("reason") {
    post.extra.insert("revision_reason".into(), reason.into());
  }
  Ok(post)
}

/// Post flair templates a subreddit offers (`flair POST '#N'` picks one).
pub async fn link_flairs(api: &Api, arg: &str) -> Result<Page<Collection>> {
  api.require_login()?;
  let name = refs::subreddit(arg)?;
  let v = api
    .get(&format!("/r/{name}/api/link_flair_v2"), &[])
    .await?;
  let items = match &v {
    Value::Array(a) => a.as_slice(),
    _ => &[],
  };
  let flairs = items
    .iter()
    .filter_map(|f| {
      let mut c = Collection {
        id: f.str("id")?,
        kind: "flair".into(),
        name: f.str("text").unwrap_or_default(),
        url: Some(sub_url(&name)),
        ..Collection::default()
      };
      for key in [
        "text_editable",
        "mod_only",
        "background_color",
        "text_color",
      ] {
        if let Some(v) = f.get(key).filter(|v| !v.is_null()) {
          c.extra.insert(key.into(), v.clone());
        }
      }
      Some(c)
    })
    .collect();
  Ok(Page::last(flairs))
}

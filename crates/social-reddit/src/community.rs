//! A subreddit's rules, moderators, wiki and post flairs.

use social_core::text::{from_secs, html_to_text};
use social_core::{Collection, Error, Page, Post, Result, User, Value, ValueExt, json};

use crate::api::{Api, WWW};
use crate::refs::{self, sub_url, user_url};

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
      let mut u = User {
        id: m.str("id").unwrap_or_else(|| user.clone()),
        url: Some(user_url(&user)),
        handle: Some(user.clone()),
        name: user,
        ..User::default()
      };
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
  let editor = d.str("revision_by.data.name").map(|n| User {
    id: d.str("revision_by.data.id").unwrap_or_else(|| n.clone()),
    url: Some(user_url(&n)),
    handle: Some(n.clone()),
    name: n,
    ..User::default()
  });
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

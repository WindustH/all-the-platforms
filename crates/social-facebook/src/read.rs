//! Reading facebook.com pages: profiles and Pages, their timelines, posts
//! with comments, groups, and (logged in) the home feed, search,
//! notifications and friends.

use std::rc::Rc;

use social_core::{
  Collection, Comment, Error, ErrorCode, Notification, Page, PageReq, Post, Result, User, Value,
  ValueExt,
};
use url::form_urlencoded::byte_serialize;

use crate::client::Client;
use crate::parse;
use crate::refs;
use crate::relay::{edges, find_all, find_key};
use crate::web::{self, HOME};

const PROFILE_OPS: [&str; 2] = [
  "ProfileCometHeaderQuery",
  "ProfilePlusCometLoggedOutRootQuery",
];
const POST_OPS: [&str; 3] = [
  "CometSinglePostDialogContentQuery",
  "CometSinglePostContentQuery",
  "CometPermalinkQuery",
];

pub async fn user(c: &Client, arg: &str) -> Result<User> {
  let page = c.page(&refs::profile_url(arg)).await?;
  page.check(&format!("profile {arg}"), &PROFILE_OPS)?;
  let u = parse::profile(&page.docs, &page.head);
  if u.id.is_empty() && u.name.is_empty() {
    return Err(Error::not_found(format!("no profile or Page `{arg}`")));
  }
  Ok(u)
}

/// A profile's timeline. Logged out Facebook shows its newest post and
/// refuses the cursor.
pub async fn user_posts(c: &Client, arg: &str, req: &PageReq) -> Result<Page<Post>> {
  let page = c.page(&refs::profile_url(arg)).await?;
  page.check(&format!("profile {arg}"), &PROFILE_OPS)?;
  let Some(cursor) = &req.cursor else {
    let (posts, next) = page
      .docs
      .values()
      .map(parse::timeline)
      .find(|(p, _)| !p.is_empty())
      .unwrap_or_default();
    return Ok(Page::new(posts, next));
  };
  let pre = page
    .preload("ProfileCometTimelineFeedQuery")
    .or_else(|| page.paged(&["Timeline"]))
    .filter(|p| p.has_cursor());
  let Some(pre) = pre else {
    return Ok(stop(c, "the timeline"));
  };
  let data = match web::advance(&c.ctx, &page, pre, cursor, req.size_within(10)).await {
    Err(e) if e.code == ErrorCode::NotAuthenticated => return Ok(stop(c, "the timeline")),
    other => other?,
  };
  let (posts, next) = parse::timeline(&data);
  Ok(Page::new(posts, next))
}

/// The end of a list Facebook continues only for logged-in visitors.
fn stop<T>(c: &Client, what: &str) -> Page<T> {
  if !c.has_session() {
    social_core::output::note(&format!(
      "Facebook continues {what} only for logged-in visitors (`social facebook login --browser`)"
    ));
  }
  Page::new(Vec::new(), None)
}

/// The story of a permalink page (posts, photos, videos, reels).
fn story(page: &web::Page) -> Option<Post> {
  for op in POST_OPS {
    let Some(d) = page.doc(op) else {
      continue;
    };
    let node = ["node_v2", "node", "story"]
      .iter()
      .map(|k| d.at(k))
      .find(|n| n.is_object())
      .or_else(|| find_key(d, "node_v2").map(|m| m.at("node_v2")));
    if let Some(n) = node {
      let p = parse::story(n);
      if !p.id.is_empty() {
        return Some(p);
      }
    }
  }
  // Other routes: the first story-shaped node anywhere.
  page.docs.values().find_map(|d| {
    let mut found = Vec::new();
    find_all(d, &is_story, &mut found);
    found
      .into_iter()
      .map(parse::story)
      .find(|p| !p.id.is_empty())
  })
}

fn is_story(v: &Value) -> bool {
  v.get("post_id").is_some_and(|p| !p.is_null()) && v.get("comet_sections").is_some()
}

pub async fn read(c: &Client, arg: &str) -> Result<Post> {
  let url = refs::post_url(arg)?;
  let page = c.page(&url).await?;
  page.check("post", &[])?;
  if let Some(mut p) = story(&page) {
    if p.url.is_none() {
      p.url = Some(page.final_url.clone());
    }
    return Ok(p);
  }
  // A video or reel page without a story: the video node itself.
  if let Some(v) = page
    .docs
    .values()
    .find_map(|d| find_key(d, "browser_native_hd_url").or_else(|| find_key(d, "playable_url")))
    && let Some(media) = parse::video(v)
  {
    return Ok(Post {
      id: v.str("id").unwrap_or_default(),
      kind: "video".into(),
      title: page.head.get("og:title").cloned(),
      text: page.head.get("og:description").cloned(),
      url: Some(page.final_url.clone()),
      author: v
        .at("owner")
        .is_object()
        .then(|| parse::user(v.at("owner"))),
      media: vec![media],
      raw: Some(v.clone()),
      ..Post::default()
    });
  }
  Err(Error::not_found(
    "the page carried no post (deleted, private, or shown only to logged-in visitors)",
  ))
}

/// Comments of a post: the first batch ships with the page, further ones
/// replay the page's comment operation (logged in).
pub async fn comments(c: &Client, arg: &str, req: &PageReq) -> Result<Page<Comment>> {
  let url = refs::post_url(arg)?;
  let page = c.page(&url).await?;
  page.check("post", &[])?;
  let post = story(&page).ok_or_else(|| Error::not_found("the page carried no post"))?;
  let author = post.author.as_ref().map(|a| a.id.clone());
  let raw = post.raw.clone().unwrap_or_default();
  let Some(cursor) = &req.cursor else {
    let (list, next) = parse::comments(&raw, author.as_deref());
    return Ok(Page::new(list, next));
  };
  let Some(pre) = page
    .preloads
    .iter()
    .find(|p| p.has_cursor() && (p.op.contains("Comment") || p.op.contains("UFI")))
  else {
    return Ok(stop(c, "these comments"));
  };
  let data = match web::advance(&c.ctx, &page, pre, cursor, req.size_within(50)).await {
    Err(e) if e.code == ErrorCode::NotAuthenticated => return Ok(stop(c, "these comments")),
    other => other?,
  };
  let (list, next) = parse::comments(&data, author.as_deref());
  Ok(Page::new(list, next))
}

/// A group's name, privacy, members and description.
pub async fn group(c: &Client, arg: &str) -> Result<Collection> {
  let page = c.page(&refs::group_url(arg)).await?;
  page.check(
    &format!("group {arg}"),
    &[
      "CometGroupRootQuery",
      "GroupsCometDiscussionLayoutRootQuery",
      "CometGroupDiscussionRootSuccessQuery",
    ],
  )?;
  let g = page
    .docs
    .values()
    .find_map(|d| {
      let g = d.at("group.profile_header_renderer.group");
      if g.is_object() {
        Some(g)
      } else {
        Some(d.at("group")).filter(|g| g.str("name").is_some())
      }
    })
    .cloned()
    .unwrap_or_default();
  let mut col = Collection {
    id: g
      .str("id")
      .or_else(|| {
        page
          .head
          .get("al:ios:url")
          .and_then(|u| u.rsplit(['/', '=']).next().map(str::to_owned))
      })
      .unwrap_or_else(|| arg.to_owned()),
    kind: "group".into(),
    name: g
      .first_str(&["name", "featurable_title.text"])
      .or_else(|| page.head.get("og:title").cloned())
      .unwrap_or_default(),
    url: g
      .str("url")
      .map(|u| web::canonical(&u))
      .or(Some(page.url.clone())),
    description: page.docs.values().find_map(|d| {
      find_key(d, "description_with_entities")
        .and_then(|m| parse::text(m.at("description_with_entities")))
    }),
    raw: Some(g.clone()),
    ..Collection::default()
  };
  if let Some(members) = g.str("group_member_profiles.formatted_count_text") {
    col.followers = members.split_whitespace().next().and_then(parse::approx);
    col.extra.insert("members_text".into(), members.into());
  }
  if let Some(privacy) = g.str("privacy_info.title.text") {
    col.extra.insert("privacy".into(), privacy.into());
  }
  Ok(col)
}

/// A group's discussion; its cursor works logged out for public groups.
pub async fn group_posts(c: &Client, arg: &str, req: &PageReq) -> Result<Page<Post>> {
  let page = c.page(&refs::group_url(arg)).await?;
  page.check(
    &format!("group {arg}"),
    &[
      "CometGroupDiscussionRootSuccessQuery",
      "CometGroupRootQuery",
    ],
  )?;
  match &req.cursor {
    None => {
      let mut posts = Vec::new();
      let mut next = None;
      if let Some(d) = page.doc("CometGroupDiscussionRootSuccessQuery") {
        if let Some(m) = find_key(d, "group_feed") {
          let (p, n) = parse::connection(m.at("group_feed"));
          posts = p;
          next = n;
        }
        if posts.is_empty() {
          let (p, _) = stories(d);
          posts = p;
        }
      }
      Ok(Page::new(posts, next))
    }
    Some(cursor) => {
      let pre = page
        .preload("CometGroupDiscussionRootSuccessQuery")
        .or_else(|| page.paged(&["Feed", "Discussion"]))
        .ok_or_else(|| {
          Error::unsupported("paging this group (no replayable operation on the page)")
        })?;
      let data = web::advance(&c.ctx, &page, pre, cursor, req.size_within(10)).await?;
      let conn = find_key(&data, "group_feed").map(|m| m.at("group_feed"));
      Ok(match conn {
        Some(conn) => {
          let (p, n) = parse::connection(conn);
          Page::new(p, n)
        }
        None => {
          let (p, n) = stories(&data);
          Page::new(p, n)
        }
      })
    }
  }
}

/// Every story in a document, and the first cursor it carries.
fn stories(d: &Value) -> (Vec<Post>, Option<String>) {
  let mut found = Vec::new();
  find_all(d, &is_story, &mut found);
  let mut seen = Vec::new();
  let posts = found
    .into_iter()
    .map(parse::story)
    .filter(|p| {
      !p.id.is_empty() && !seen.contains(&p.id) && {
        seen.push(p.id.clone());
        true
      }
    })
    .collect();
  let cursor = find_key(d, "end_cursor").and_then(|m| m.str("end_cursor"));
  (posts, cursor)
}

fn require_session(c: &Client, what: &str) -> Result<()> {
  if c.has_session() {
    return Ok(());
  }
  Err(
    Error::auth(format!("{what} needs a logged-in Facebook session"))
      .with_hint("run `social facebook login --browser` (or `--cookie 'c_user=…; xs=…'`)"),
  )
}

/// A logged-in page's stories, then its paged feed operation.
async fn session_feed(c: &Client, url: &str, ops: &[&str], req: &PageReq) -> Result<Page<Post>> {
  let page: Rc<web::Page> = c.page(url).await?;
  page.check("this page", &[])?;
  match &req.cursor {
    None => {
      let mut posts = Vec::new();
      let mut next = None;
      for d in page.docs.values() {
        let (p, n) = stories(d);
        if !p.is_empty() {
          posts.extend(p);
          next = next.or(n);
        }
      }
      Ok(Page::new(posts, next))
    }
    Some(cursor) => {
      let pre = page.paged(ops).ok_or_else(|| {
        Error::unsupported("paging this list (no replayable operation on the page)")
      })?;
      let data = web::advance(&c.ctx, &page, pre, cursor, req.size_within(10)).await?;
      let (p, n) = stories(&data);
      Ok(Page::new(p, n))
    }
  }
}

pub async fn feed(c: &Client, req: &PageReq) -> Result<Page<Post>> {
  require_session(c, "the home feed")?;
  session_feed(c, &format!("{HOME}/"), &["NewsFeed", "Feed"], req).await
}

pub async fn search(c: &Client, query: &str, req: &PageReq) -> Result<Page<Post>> {
  require_session(c, "search")?;
  let q: String = byte_serialize(query.as_bytes()).collect();
  session_feed(c, &format!("{HOME}/search/posts/?q={q}"), &["Search"], req).await
}

/// People or Pages found by name (logged in).
pub async fn search_users(c: &Client, query: &str) -> Result<Page<User>> {
  require_session(c, "search")?;
  let q: String = byte_serialize(query.as_bytes()).collect();
  let page = c.page(&format!("{HOME}/search/people/?q={q}")).await?;
  page.check("search", &[])?;
  Ok(Page::last(people(&page)))
}

/// Friends of a profile (logged in; as far as Facebook shows them).
pub async fn friends(c: &Client, arg: Option<&str>) -> Result<Page<User>> {
  require_session(c, "friend lists")?;
  let me = c.ctx.http.cookie("c_user").unwrap_or_default();
  let who = arg.unwrap_or(&me);
  let page = c.page(&refs::tab_url(who, "friends")).await?;
  page.check("friends", &[])?;
  let owner = parse::profile(&page.docs, &page.head).id;
  Ok(Page::last(
    people(&page)
      .into_iter()
      .filter(|u| u.id != owner && u.id != me)
      .collect(),
  ))
}

/// Distinct user-like nodes (an id, a name and a profile link) of a page.
fn people(page: &web::Page) -> Vec<User> {
  let mut out: Vec<User> = Vec::new();
  for d in page.docs.values() {
    let mut found = Vec::new();
    let pred = |v: &Value| {
      matches!(v.str("__typename").as_deref(), Some("User" | "Page"))
        && v.str("name").is_some()
        && v.first_str(&["url", "profile_url"]).is_some()
    };
    find_all(d, &pred, &mut found);
    for n in found {
      let u = parse::user(n);
      if !u.id.is_empty() && !out.iter().any(|o| o.id == u.id) {
        out.push(u);
      }
    }
  }
  out
}

pub async fn notifications(c: &Client) -> Result<Page<Notification>> {
  require_session(c, "notifications")?;
  let page = c.page(&format!("{HOME}/notifications")).await?;
  page.check("notifications", &[])?;
  let mut out: Vec<Notification> = Vec::new();
  for d in page.docs.values() {
    for e in find_key(d, "notifications")
      .map(|m| edges(m.at("notifications")))
      .unwrap_or_default()
    {
      let n = if e.at("notif").is_object() {
        e.at("notif")
      } else {
        e
      };
      let id = n
        .str("notif_id")
        .or_else(|| n.str("id"))
        .unwrap_or_default();
      if id.is_empty() || out.iter().any(|o| o.id == id) {
        continue;
      }
      out.push(Notification {
        id,
        kind: n.str("notif_type").unwrap_or_else(|| "notification".into()),
        text: n.str("body.text").unwrap_or_default(),
        url: n.str("url").map(|u| web::canonical(&u)),
        created_at: n
          .i64("creation_time.timestamp")
          .or_else(|| n.i64("creation_time"))
          .and_then(social_core::text::from_secs),
        unread: n.str("seen_state").map(|s| s != "SEEN_AND_READ"),
        raw: Some(n.clone()),
        ..Notification::default()
      });
    }
  }
  Ok(Page::last(out))
}

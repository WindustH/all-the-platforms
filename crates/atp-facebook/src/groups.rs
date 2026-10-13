//! Groups: their name, privacy, members and description, and their
//! discussion.

use atp_core::{Collection, Error, Page, PageReq, Post, Result, Value, ValueExt};

use crate::api::Api;
use crate::parse;
use crate::refs::{self, canonical};
use crate::relay::find_key;
use crate::web::{self, advance};

const DISCUSSION_OP: &str = "CometGroupDiscussionRootSuccessQuery";

/// A group's name, privacy, members and description.
pub async fn group(api: &Api, arg: &str) -> Result<Collection> {
  let page = api.page(&refs::group_url(arg)).await?;
  page.check(
    &format!("group {arg}"),
    &[
      "CometGroupRootQuery",
      "GroupsCometDiscussionLayoutRootQuery",
      DISCUSSION_OP,
    ],
  )?;
  let node = page
    .docs
    .values()
    .find_map(group_node)
    .cloned()
    .unwrap_or_default();
  let mut group = Collection {
    id: node
      .str("id")
      .or_else(|| {
        page
          .head
          .get("al:ios:url")
          .and_then(|u| u.rsplit(['/', '=']).next().map(str::to_owned))
      })
      .unwrap_or_else(|| arg.to_owned()),
    kind: "group".into(),
    name: node
      .first_str(&["name", "featurable_title.text"])
      .or_else(|| page.head.get("og:title").cloned())
      .unwrap_or_default(),
    url: node
      .str("url")
      .map(|u| canonical(&u))
      .or(Some(page.url.clone())),
    description: description(&page),
    raw: Some(node.clone()),
    ..Collection::default()
  };
  if let Some(members) = node.str("group_member_profiles.formatted_count_text") {
    group.followers = members.split_whitespace().next().and_then(parse::approx);
    group.extra.insert("members_text".into(), members.into());
  }
  if let Some(privacy) = node.str("privacy_info.title.text") {
    group.extra.insert("privacy".into(), privacy.into());
  }
  Ok(group)
}

/// The group node of a document: its header's, else a named `group`.
fn group_node(doc: &Value) -> Option<&Value> {
  let header = doc.at("group.profile_header_renderer.group");
  if header.is_object() {
    return Some(header);
  }
  Some(doc.at("group")).filter(|g| g.str("name").is_some())
}

fn description(page: &web::Page) -> Option<String> {
  page.docs.values().find_map(|doc| {
    find_key(doc, "description_with_entities")
      .and_then(|m| parse::text(m.at("description_with_entities")))
  })
}

/// A group's discussion; its cursor works logged out for public groups.
pub async fn group_posts(api: &Api, arg: &str, req: &PageReq) -> Result<Page<Post>> {
  let page = api.page(&refs::group_url(arg)).await?;
  page.check(
    &format!("group {arg}"),
    &[DISCUSSION_OP, "CometGroupRootQuery"],
  )?;
  let Some(cursor) = &req.cursor else {
    let Some(doc) = page.doc(DISCUSSION_OP) else {
      return Ok(Page::new(Vec::new(), None));
    };
    let (mut posts, next) = discussion(doc).unwrap_or_default();
    if posts.is_empty() {
      posts = parse::stories(doc).0;
    }
    return Ok(Page::new(posts, next));
  };
  let operation = page
    .preload(DISCUSSION_OP)
    .or_else(|| page.paged(&["Feed", "Discussion"]))
    .ok_or_else(|| Error::unsupported("paging this group (no replayable operation on the page)"))?;
  let data = advance(&api.ctx, &page, operation, cursor, req.size_within(10)).await?;
  let (posts, next) = discussion(&data).unwrap_or_else(|| parse::stories(&data));
  Ok(Page::new(posts, next))
}

/// The stories of a document's `group_feed` connection, when it has one.
fn discussion(doc: &Value) -> Option<(Vec<Post>, Option<String>)> {
  find_key(doc, "group_feed").map(|m| parse::connection(m.at("group_feed")))
}

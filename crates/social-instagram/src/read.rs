//! Reading: profiles, posts, comments, likers, follower lists, search,
//! hashtag and location feeds, saved posts and collections, liked posts,
//! the activity inbox, stories and highlights.

use social_core::text::from_secs;
use social_core::{
  Collection, Comment, Error, Notification, Page, PageReq, Post, Result, User, Value, ValueExt,
  json,
};

use crate::client::Client;
use crate::parse;
use crate::refs;

/// PolarisPostActionLoadPostQueryQuery: one post by shortcode, anonymously.
const POST_DOC: &str = "8845758582119845";

/// A profile by username (anonymous) or numeric id (logged in).
pub async fn profile(c: &Client, arg: &str) -> Result<(User, Value)> {
  match refs::username(arg) {
    Some(name) => {
      let v = c
        .get("users/web_profile_info/", &[("username", name.clone())])
        .await?;
      let u = v.at("data.user");
      if !u.is_object() {
        return Err(Error::not_found(format!("no Instagram user @{name}")));
      }
      Ok((parse::user(u), u.clone()))
    }
    None => {
      c.require_session("looking up a user by id")?;
      let v = c.get(&format!("users/{}/info/", arg.trim()), &[]).await?;
      Ok((parse::user(v.at("user")), v.at("user").clone()))
    }
  }
}

/// The numeric id of a user argument.
pub async fn user_id(c: &Client, arg: &str) -> Result<String> {
  if refs::username(arg).is_none() {
    return Ok(arg.trim().to_owned());
  }
  Ok(profile(c, arg).await?.0.id)
}

pub async fn user(c: &Client, arg: &str) -> Result<User> {
  Ok(profile(c, arg).await?.0)
}

/// A user's posts: the v1 feed when logged in, the first twelve otherwise.
pub async fn user_posts(c: &Client, arg: &str, req: &PageReq) -> Result<Page<Post>> {
  if !c.has_session() {
    if req.cursor.is_some() {
      return Err(
        Error::auth("more than the first posts needs a logged-in session")
          .with_hint("run `social instagram login --browser`"),
      );
    }
    let (_, raw) = profile(c, arg).await?;
    let posts = raw
      .list("edge_owner_to_timeline_media.edges")
      .iter()
      .map(|e| parse::media(e.at("node")))
      .collect();
    return Ok(Page::new(posts, None));
  }
  let id = user_id(c, arg).await?;
  let mut q = vec![("count", req.size_within(33).to_string())];
  if let Some(cur) = &req.cursor {
    q.push(("max_id", cur.clone()));
  }
  let v = c.get(&format!("feed/user/{id}/"), &q).await?;
  Ok(items_page(&v, "items"))
}

fn items_page(v: &Value, key: &str) -> Page<Post> {
  let posts = v
    .list(key)
    .iter()
    .map(|i| {
      parse::media(if i.at("media").is_object() {
        i.at("media")
      } else {
        i
      })
    })
    .collect();
  let next = (v.bool("more_available") != Some(false))
    .then(|| v.str("next_max_id"))
    .flatten();
  Page::new(posts, next)
}

/// One post: v1 media info when logged in, the public GraphQL query otherwise.
pub async fn read(c: &Client, arg: &str) -> Result<Post> {
  if c.has_session() {
    let id = refs::media_id(arg)?;
    let v = c.get(&format!("media/{id}/info/"), &[]).await?;
    let item = v
      .list("items")
      .first()
      .ok_or_else(|| Error::not_found("no such post"))?;
    return Ok(parse::media(item));
  }
  Ok(parse::media(&public_post(c, arg).await?))
}

async fn public_post(c: &Client, arg: &str) -> Result<Value> {
  let code = match refs::shortcode(arg) {
    Ok(code) => code,
    Err(_) => refs::encode(&refs::media_id(arg)?).ok_or_else(|| Error::input("bad post id"))?,
  };
  let vars = json!({
    "shortcode": code,
    "fetch_tagged_user_count": null,
    "hoisted_comment_id": null,
    "hoisted_reply_id": null,
  });
  let v = c.graphql(POST_DOC, &vars).await?;
  let m = v.at("data.xdt_shortcode_media");
  if !m.is_object() {
    return Err(Error::not_found(format!(
      "post {code} is not public (or does not exist)"
    )));
  }
  Ok(m.clone())
}

/// Comments, newest batch first; logged out only the ones the post page ships.
pub async fn comments(c: &Client, arg: &str, req: &PageReq) -> Result<Page<Comment>> {
  if !c.has_session() {
    let m = public_post(c, arg).await?;
    let list = m
      .list("edge_media_to_parent_comment.edges")
      .iter()
      .map(|e| parse::comment(e.at("node")))
      .collect();
    return Ok(Page::new(list, None));
  }
  let id = refs::media_id(arg)?;
  let mut q = vec![
    ("can_support_threading", "true".to_owned()),
    ("permalink_enabled", "false".to_owned()),
  ];
  if let Some(cur) = &req.cursor {
    q.push(("min_id", cur.clone()));
  }
  let v = c.get(&format!("media/{id}/comments/"), &q).await?;
  let list = v.list("comments").iter().map(parse::comment).collect();
  let next = v.str("next_min_id").filter(|_| {
    v.bool("has_more_headload_comments") != Some(false) || v.str("next_min_id").is_some()
  });
  Ok(Page::new(list, next))
}

pub async fn replies(c: &Client, arg: &str, comment: &str, req: &PageReq) -> Result<Page<Comment>> {
  c.require_session("comment replies")?;
  let id = refs::media_id(arg)?;
  let mut q = Vec::new();
  if let Some(cur) = &req.cursor {
    q.push(("max_id", cur.clone()));
  }
  let v = c
    .get(
      &format!("media/{id}/comments/{comment}/child_comments/"),
      &q,
    )
    .await?;
  let list = v
    .list("child_comments")
    .iter()
    .map(parse::comment)
    .collect();
  let next = (v.bool("has_more_tail_child_comments") == Some(true))
    .then(|| v.str("next_max_child_cursor"))
    .flatten();
  Ok(Page::new(list, next))
}

pub async fn likers(c: &Client, arg: &str) -> Result<Page<User>> {
  c.require_session("likers")?;
  let id = refs::media_id(arg)?;
  let v = c.get(&format!("media/{id}/likers/"), &[]).await?;
  Ok(Page::last(
    v.list("users").iter().map(parse::user).collect(),
  ))
}

/// Followers or following of a user.
pub async fn follows(c: &Client, arg: &str, edge: &str, req: &PageReq) -> Result<Page<User>> {
  c.require_session("follower lists")?;
  let id = user_id(c, arg).await?;
  let mut q = vec![("count", req.size_within(50).to_string())];
  if let Some(cur) = &req.cursor {
    q.push(("max_id", cur.clone()));
  }
  let v = c.get(&format!("friendships/{id}/{edge}/"), &q).await?;
  let users = v.list("users").iter().map(parse::user).collect();
  Ok(Page::new(users, v.str("next_max_id")))
}

/// Top search: accounts, hashtags and places matching a word.
async fn topsearch(c: &Client, query: &str) -> Result<Value> {
  c.get(
    "web/search/topsearch/",
    &[
      ("query", query.to_owned()),
      ("context", "blended".to_owned()),
    ],
  )
  .await
}

pub async fn search_users(c: &Client, query: &str) -> Result<Page<User>> {
  let v = topsearch(c, query).await?;
  Ok(Page::last(
    v.list("users")
      .iter()
      .map(|u| parse::user(u.at("user")))
      .collect(),
  ))
}

pub async fn search_topics(c: &Client, query: &str) -> Result<Page<Collection>> {
  let v = topsearch(c, query).await?;
  let tags = v.list("hashtags").iter().map(|h| {
    let h = h.at("hashtag");
    let name = h.str("name").unwrap_or_default();
    Collection {
      id: name.clone(),
      kind: "hashtag".into(),
      url: Some(format!("https://www.instagram.com/explore/tags/{name}/")),
      items: h.count("media_count"),
      name: format!("#{name}"),
      ..Collection::default()
    }
  });
  let places = v.list("places").iter().map(|p| {
    let l = p.at("place.location");
    let id = l.str("pk").unwrap_or_default();
    Collection {
      kind: "location".into(),
      name: l.str("name").unwrap_or_default(),
      description: l.str("address").filter(|a| !a.is_empty()),
      url: Some(format!("https://www.instagram.com/explore/locations/{id}/")),
      id,
      ..Collection::default()
    }
  });
  Ok(Page::last(tags.chain(places).collect()))
}

/// Posts of a hashtag (`top` or `recent`).
pub async fn tag(c: &Client, tag: &str, recent: bool, req: &PageReq) -> Result<Page<Post>> {
  let tag = refs::tag(tag);
  let tab = if recent { "recent" } else { "top" };
  let v = match &req.cursor {
    None => {
      let v = c
        .get("tags/web_info/", &[("tag_name", tag.clone())])
        .await?;
      v.at(&format!("data.{tab}")).clone()
    }
    Some(cur) => {
      let (max_id, page) = cur.split_once('|').unwrap_or((cur, "1"));
      c.post(
        &format!("tags/{tag}/sections/"),
        &[
          ("tab", tab.to_owned()),
          ("max_id", max_id.to_owned()),
          ("page", page.to_owned()),
          ("include_persistent", "0".to_owned()),
          ("surface", "grid".to_owned()),
        ],
      )
      .await?
    }
  };
  Ok(sections(&v))
}

pub async fn location(c: &Client, loc: &str, recent: bool, req: &PageReq) -> Result<Page<Post>> {
  let id = refs::location(loc);
  let tab = if recent { "recent" } else { "ranked" };
  let v = match &req.cursor {
    None => {
      let v = c
        .get("locations/web_info/", &[("location_id", id.clone())])
        .await?;
      let data = v.at("native_location_data");
      data.at(if recent { "recent" } else { "ranked" }).clone()
    }
    Some(cur) => {
      let (max_id, page) = cur.split_once('|').unwrap_or((cur, "1"));
      c.post(
        &format!("locations/{id}/sections/"),
        &[
          ("tab", tab.to_owned()),
          ("max_id", max_id.to_owned()),
          ("page", page.to_owned()),
          ("surface", "grid".to_owned()),
        ],
      )
      .await?
    }
  };
  Ok(sections(&v))
}

/// Grid sections (`layout_content.medias[].media`) and their `max_id|page` cursor.
fn sections(v: &Value) -> Page<Post> {
  let posts = v
    .list("sections")
    .iter()
    .flat_map(|s| {
      s.list("layout_content.medias")
        .iter()
        .chain(s.list("layout_content.fill_items"))
        .map(|m| parse::media(m.at("media")))
        .collect::<Vec<_>>()
    })
    .collect();
  let next = (v.bool("more_available") == Some(true))
    .then(|| {
      v.str("next_max_id")
        .map(|m| format!("{m}|{}", v.str("next_page").unwrap_or_else(|| "1".into())))
    })
    .flatten();
  Page::new(posts, next)
}

/// Saved posts, or those of one collection.
pub async fn saved(c: &Client, folder: Option<&str>, req: &PageReq) -> Result<Page<Post>> {
  c.require_session("saved posts")?;
  let path = match folder {
    Some(f) => format!("feed/collection/{f}/posts/"),
    None => "feed/saved/posts/".to_owned(),
  };
  let mut q = Vec::new();
  if let Some(cur) = &req.cursor {
    q.push(("max_id", cur.clone()));
  }
  let v = c.get(&path, &q).await?;
  Ok(items_page(&v, "items"))
}

pub async fn collections(c: &Client) -> Result<Page<Collection>> {
  c.require_session("collections")?;
  let v = c
    .get(
      "collections/list/",
      &[(
        "collection_types",
        r#"["ALL_MEDIA_AUTO_COLLECTION","MEDIA","AUDIO_AUTO_COLLECTION"]"#.to_owned(),
      )],
    )
    .await?;
  Ok(Page::last(
    v.list("items")
      .iter()
      .map(|i| Collection {
        id: i.str("collection_id").unwrap_or_default(),
        kind: "folder".into(),
        name: i.str("collection_name").unwrap_or_default(),
        items: i.count("collection_media_count"),
        ..Collection::default()
      })
      .collect(),
  ))
}

pub async fn liked(c: &Client, req: &PageReq) -> Result<Page<Post>> {
  c.require_session("liked posts")?;
  let mut q = Vec::new();
  if let Some(cur) = &req.cursor {
    q.push(("max_id", cur.clone()));
  }
  let v = c.get("feed/liked/", &q).await?;
  Ok(items_page(&v, "items"))
}

/// The activity inbox: likes, comments, follows, mentions.
pub async fn notifications(c: &Client) -> Result<Page<Notification>> {
  c.require_session("notifications")?;
  let v = c.get("news/inbox/", &[]).await?;
  let mut out = Vec::new();
  for (unread, key) in [(true, "new_stories"), (false, "old_stories")] {
    for s in v.list(key) {
      let a = s.at("args");
      let kind = match s.i64("story_type") {
        Some(60) => "like",
        Some(12 | 13 | 66) => "comment",
        Some(101) => "follow",
        Some(102) => "mention",
        _ => "activity",
      };
      out.push(Notification {
        id: s.str("pk").or_else(|| a.str("tuuid")).unwrap_or_default(),
        kind: kind.into(),
        text: a.str("text").unwrap_or_default(),
        actor: a.str("profile_name").map(|name| social_core::User {
          id: a.str("profile_id").unwrap_or_default(),
          url: Some(format!("https://www.instagram.com/{name}/")),
          handle: Some(name.clone()),
          name,
          avatar: a.str("profile_image"),
          ..social_core::User::default()
        }),
        target: a.str("media.0.shortcode").map(|c| refs::post_url(&c)),
        url: a.str("destination"),
        created_at: a.f64("timestamp").map(|t| t as i64).and_then(from_secs),
        unread: Some(unread),
        raw: Some(s.clone()),
      });
    }
  }
  Ok(Page::last(out))
}

/// Current stories of a user.
pub async fn stories(c: &Client, arg: &str) -> Result<Page<Post>> {
  c.require_session("stories")?;
  let id = user_id(c, arg).await?;
  reel(c, &id).await
}

/// One reel tray: a user's stories or a highlight (`highlight:ID`).
pub async fn reel(c: &Client, reel_id: &str) -> Result<Page<Post>> {
  let v = c
    .get("feed/reels_media/", &[("reel_ids", reel_id.to_owned())])
    .await?;
  let items = v
    .at("reels")
    .get(reel_id)
    .or_else(|| v.list("reels_media").first())
    .map(|r| r.list("items").to_vec())
    .unwrap_or_default();
  Ok(Page::last(
    items
      .iter()
      .map(|i| {
        let mut p = parse::media(i);
        p.kind = "story".into();
        p
      })
      .collect(),
  ))
}

pub async fn highlights(c: &Client, arg: &str) -> Result<Page<Collection>> {
  c.require_session("highlights")?;
  let id = user_id(c, arg).await?;
  let v = c
    .get(&format!("highlights/{id}/highlights_tray/"), &[])
    .await?;
  Ok(Page::last(
    v.list("tray")
      .iter()
      .map(|h| Collection {
        id: h.str("id").unwrap_or_default(),
        kind: "highlight".into(),
        name: h.str("title").unwrap_or_default(),
        items: h.count("media_count"),
        ..Collection::default()
      })
      .collect(),
  ))
}

/// The logged-in account.
pub async fn whoami(c: &Client) -> Result<User> {
  c.require_session("whoami")?;
  let id = c
    .my_id()
    .ok_or_else(|| Error::auth("the session has no ds_user_id cookie"))?;
  let v = c.get(&format!("users/{id}/info/"), &[]).await?;
  Ok(parse::user(v.at("user")))
}

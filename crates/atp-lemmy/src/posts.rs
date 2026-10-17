//! Posts: listings (hot, feeds, a community's), one post, votes, saving,
//! writing and deleting. Arguments of other instances are looked up
//! through this one (`resolve_object`).

use atp_core::{Action, Draft, Error, Page, PageReq, Post, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::parse;
use crate::refs::{self, Target};

/// `hot --category` and `--sort` of listings: Lemmy's sorts, in kebab case.
pub const SORT_NAMES: &[&str] = &[
  "hot",
  "active",
  "scaled",
  "new",
  "old",
  "controversial",
  "most-comments",
  "new-comments",
  "top-hour",
  "top-day",
  "top-week",
  "top-month",
  "top-year",
  "top-all",
];
/// `feed --type`.
pub const FEEDS: &[&str] = &["subscribed", "local", "all"];
/// The most posts one request lists.
const PAGE_MAX: usize = 50;

/// Lemmy's name of a sort given on the command line (`top-day` → `TopDay`).
pub fn sort(name: Option<&str>, default: &str) -> Result<String> {
  let name = name.unwrap_or(default);
  if !SORT_NAMES.contains(&name) {
    return Err(Error::input(format!("unknown sort: {name}")));
  }
  Ok(
    name
      .split('-')
      .map(|word| {
        let mut chars = word.chars();
        chars
          .next()
          .map(|first| first.to_ascii_uppercase().to_string() + chars.as_str())
          .unwrap_or_default()
      })
      .collect(),
  )
}

/// One page of `post/list`: Lemmy's `next_page` cursor, or page numbers
/// (`page:N`) where an instance hands out none.
async fn list(api: &Api, mut params: Vec<(&str, String)>, page: &PageReq) -> Result<Page<Post>> {
  let limit = page.size_within(PAGE_MAX);
  params.push(("limit", limit.to_string()));
  let number = match page.cursor.as_deref() {
    Some(cursor) => match cursor.strip_prefix("page:") {
      Some(n) => n.parse().unwrap_or(1),
      None => {
        params.push(("page_cursor", cursor.to_owned()));
        0
      }
    },
    None => 1,
  };
  if number > 0 {
    params.push(("page", number.to_string()));
  }
  let v = api.get("post/list", params).await?;
  let rows = v.list("posts");
  let instance = api.instance();
  let next = v
    .str("next_page")
    .or_else(|| (number > 0 && rows.len() >= limit).then(|| format!("page:{}", number + 1)))
    .filter(|_| !rows.is_empty());
  Ok(Page::new(
    rows.iter().map(|row| parse::post(row, &instance)).collect(),
    next,
  ))
}

pub async fn hot(api: &Api, category: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let params = vec![
    ("type_", "All".to_owned()),
    ("sort", sort(category, "hot")?),
  ];
  list(api, params, page).await
}

/// Your subscriptions (needs an account), the instance's own posts, or all.
pub async fn feed(api: &Api, kind: Option<&str>, page: &PageReq) -> Result<Page<Post>> {
  let default = if api.token().is_some() {
    "subscribed"
  } else {
    "local"
  };
  let listing = match kind.unwrap_or(default) {
    "subscribed" => {
      api.require()?;
      "Subscribed"
    }
    "local" => "Local",
    _ => "All",
  };
  let params = vec![("type_", listing.to_owned()), ("sort", "Hot".to_owned())];
  list(api, params, page).await
}

pub async fn community(
  api: &Api,
  arg: &str,
  sort_name: Option<&str>,
  page: &PageReq,
) -> Result<Page<Post>> {
  let name = refs::community(arg, &api.instance())?;
  let params = vec![
    ("community_name", name),
    ("type_", "All".to_owned()),
    ("sort", sort(sort_name, "hot")?),
  ];
  list(api, params, page).await
}

/// Posts you saved (Lemmy shows nobody else's, and has no folders).
pub async fn saved(
  api: &Api,
  user: Option<&str>,
  folder: Option<&str>,
  page: &PageReq,
) -> Result<Page<Post>> {
  if user.is_some() || folder.is_some() {
    return Err(
      Error::unsupported("favorites USER / --folder")
        .with_hint("Lemmy shows only your own saved posts"),
    );
  }
  api.require()?;
  let params = vec![
    ("saved_only", "true".to_owned()),
    ("type_", "All".to_owned()),
    ("sort", "New".to_owned()),
  ];
  list(api, params, page).await
}

/// What a link of another instance is on this one.
pub async fn resolve(api: &Api, link: &str) -> Result<Target> {
  let v = api
    .get("resolve_object", vec![("q", link.to_owned())])
    .await
    .map_err(|e| {
      if api.token().is_none() {
        e.with_hint(
          "links of other instances are looked up through yours, which may need an account",
        )
      } else {
        e
      }
    })?;
  if let Some(id) = v.u64("post.post.id") {
    return Ok(Target::Post(id));
  }
  if let Some(id) = v.u64("comment.comment.id") {
    return Ok(Target::Comment(id));
  }
  Err(Error::not_found(format!(
    "{link} is no post or comment {} knows",
    api.instance()
  )))
}

/// A post or comment argument, links of other instances resolved.
pub async fn target(api: &Api, arg: &str) -> Result<Target> {
  match refs::target(arg, &api.instance())? {
    Target::Remote(link) => resolve(api, &link).await,
    local => Ok(local),
  }
}

/// The post of an argument; a comment stands for its post.
pub async fn post_id(api: &Api, arg: &str) -> Result<u64> {
  match target(api, arg).await? {
    Target::Post(id) => Ok(id),
    Target::Comment(id) => {
      let v = api.get("comment", vec![("id", id.to_string())]).await?;
      v.u64("comment_view.comment.post_id")
        .ok_or_else(|| Error::not_found(format!("no post for comment {id}")))
    }
    Target::Remote(link) => Err(Error::not_found(format!("cannot resolve {link}"))),
  }
}

pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let id = post_id(api, arg).await?;
  let v = api.get("post", vec![("id", id.to_string())]).await?;
  let mut post = parse::post(v.at("post_view"), &api.instance());
  let crossposts: Vec<Value> = v
    .list("cross_posts")
    .iter()
    .map(|c| {
      json!(format!(
        "https://{}/post/{}",
        api.instance(),
        c.str("post.id").unwrap_or_default()
      ))
    })
    .collect();
  if !crossposts.is_empty() {
    post.extra.insert("cross_posts".into(), json!(crossposts));
  }
  Ok(post)
}

/// Vote on a post: 1 up, -1 down, 0 to take a vote back.
pub async fn vote(api: &Api, arg: &str, score: i8) -> Result<Action> {
  let action = match score {
    1 => "upvote",
    -1 => "downvote",
    _ => "unvote",
  };
  match target(api, arg).await? {
    Target::Comment(id) => {
      api
        .write("comment/like", json!({"comment_id": id, "score": score}))
        .await?;
    }
    Target::Post(id) => {
      api
        .write("post/like", json!({"post_id": id, "score": score}))
        .await?;
    }
    Target::Remote(link) => return Err(Error::not_found(format!("cannot resolve {link}"))),
  }
  Ok(Action::done(action, arg))
}

/// A post argument, never a comment (writes that act on posts only).
async fn own_post(api: &Api, arg: &str) -> Result<u64> {
  match target(api, arg).await? {
    Target::Post(id) => Ok(id),
    _ => Err(
      Error::input(format!("{arg} is not a post"))
        .with_hint("comments have their own commands: vote, delete-comment"),
    ),
  }
}

pub async fn save(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let id = own_post(api, arg).await?;
  api
    .put("post/save", json!({"post_id": id, "save": !undo}))
    .await?;
  Ok(Action::done(if undo { "unsave" } else { "save" }, arg))
}

/// What `submit` sends.
pub struct Submission {
  pub community: String,
  pub title: String,
  pub link: Option<String>,
  pub body: Option<String>,
  pub nsfw: bool,
}

pub async fn submit(api: &Api, post: Submission) -> Result<Action> {
  api.require()?;
  let name = refs::community(&post.community, &api.instance())?;
  let v = api.get("community", vec![("name", name.clone())]).await?;
  let community_id = v
    .u64("community_view.community.id")
    .ok_or_else(|| Error::not_found(format!("no community {name}")))?;
  let mut body = json!({"name": post.title, "community_id": community_id, "nsfw": post.nsfw});
  if let Some(link) = post.link {
    body["url"] = json!(link);
  }
  if let Some(text) = post.body.filter(|text| !text.is_empty()) {
    body["body"] = json!(text);
  }
  let v = api.write("post", body).await?;
  let id = v.str("post_view.post.id").unwrap_or_default();
  Ok(
    Action::done("post", name)
      .with_id(id.clone())
      .with_url(format!("https://{}/post/{id}", api.instance())),
  )
}

/// The shared `post`: a title, the text, and the community as `--topic`.
pub async fn publish(api: &Api, draft: &Draft) -> Result<Action> {
  if !draft.images.is_empty() || draft.reply_to.is_some() || draft.quote.is_some() {
    return Err(
      Error::unsupported("post --image / --reply-to / --quote")
        .with_hint("Lemmy posts take a title, text and a link: see `atp lemmy submit`"),
    );
  }
  let [community] = draft.topics.as_slice() else {
    return Err(Error::input(
      "name the community with --topic (one, e.g. --topic '!rust@lemmy.ml')",
    ));
  };
  let title = draft
    .title
    .clone()
    .ok_or_else(|| Error::input("Lemmy posts need --title"))?;
  let post = Submission {
    community: community.clone(),
    title,
    link: None,
    body: Some(draft.text.clone()),
    nsfw: false,
  };
  submit(api, post).await
}

pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  let id = own_post(api, arg).await?;
  api
    .write("post/delete", json!({"post_id": id, "deleted": true}))
    .await?;
  Ok(Action::done("delete", arg))
}

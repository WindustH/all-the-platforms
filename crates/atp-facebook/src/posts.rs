//! Posts: reading a facebook.com permalink or a Page post of the Graph API,
//! and publishing to, editing on and deleting from a Page.

use std::path::PathBuf;

use atp_core::cli::Change;
use atp_core::http::Part;
use atp_core::{Action, Draft, Edit, Error, ErrorCode, Post, Result, ValueExt, json};
use atp_meta::graph::{Graph, Params, fields};

use crate::api::{Api, photo_part};
use crate::parse;
use crate::parse::graph::POST_FIELDS;
use crate::refs::{self, graph_id, is_graph_id};
use crate::relay::find_key;
use crate::web;

/// A post by Graph id (`PAGE_POST`) or by link.
pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  if is_graph_id(arg) {
    return read_graph(api, arg).await;
  }
  let page = api.page(&refs::post_url(arg)?).await?;
  page.check("post", &[])?;
  if let Some(mut post) = parse::permalink(&page.docs) {
    if post.url.is_none() {
      post.url = Some(page.final_url.clone());
    }
    return Ok(post);
  }
  video_post(&page).ok_or_else(|| {
    Error::not_found(
      "the page carried no post (deleted, private, or shown only to logged-in visitors)",
    )
  })
}

/// A Page post through the Graph API.
pub async fn read_graph(api: &Api, arg: &str) -> Result<Post> {
  let id = graph_id(arg)?;
  let graph = api.graph_for(&id).await?;
  Ok(parse::graph::post(
    &graph.get(&id, &fields(POST_FIELDS)).await?,
  ))
}

/// A video or reel page without a story: the video node itself.
fn video_post(page: &web::Page) -> Option<Post> {
  let node = page.docs.values().find_map(|doc| {
    find_key(doc, "browser_native_hd_url").or_else(|| find_key(doc, "playable_url"))
  })?;
  let media = parse::video(node)?;
  Some(Post {
    id: node.str("id").unwrap_or_default(),
    kind: "video".into(),
    title: page.head.get("og:title").cloned(),
    text: page.head.get("og:description").cloned(),
    url: Some(page.final_url.clone()),
    author: node
      .at("owner")
      .is_object()
      .then(|| parse::user(node.at("owner"))),
    media: vec![media],
    raw: Some(node.clone()),
    ..Post::default()
  })
}

/// Publish to a Page: text (`message`), one photo, or several photos
/// uploaded unpublished and attached to one post.
pub async fn publish(api: &Api, draft: &Draft) -> Result<Action> {
  if draft.reply_to.is_some() || draft.quote.is_some() {
    return Err(Error::unsupported(
      "post --reply-to / --quote (use `comment` on Pages)",
    ));
  }
  let page = api
    .page_id(draft.topics.first().map(String::as_str))
    .await?;
  let graph = api.page_graph(&page).await?;
  let message = message(draft);
  let v = match draft.images.as_slice() {
    [] => {
      let form = vec![("message".to_owned(), message)];
      graph.post(&format!("{page}/feed"), &form).await?
    }
    [one] => {
      let parts = vec![photo_part(one).await?, Part::text("message", message)];
      graph
        .post_multipart(&format!("{page}/photos"), parts)
        .await?
    }
    many => album(&graph, &page, message, many).await?,
  };
  let id = v.str("post_id").or_else(|| v.str("id")).unwrap_or_default();
  Ok(
    Action::done("publish", &page)
      .with_url(format!("https://www.facebook.com/{id}"))
      .with_id(id),
  )
}

/// The title above the text, as one message.
fn message(draft: &Draft) -> String {
  match &draft.title {
    Some(title) if draft.text.is_empty() => title.clone(),
    Some(title) => format!("{title}\n\n{}", draft.text),
    None => draft.text.clone(),
  }
}

/// Several photos: uploaded unpublished, then attached to one feed post.
async fn album(
  graph: &Graph<'_>,
  page: &str,
  message: String,
  images: &[PathBuf],
) -> Result<atp_core::Value> {
  let mut form: Params = vec![("message".to_owned(), message)];
  for (i, path) in images.iter().enumerate() {
    let parts = vec![photo_part(path).await?, Part::text("published", "false")];
    let upload = graph
      .post_multipart(&format!("{page}/photos"), parts)
      .await?;
    let id = upload
      .str("id")
      .ok_or_else(|| Error::upstream("photo upload returned no id"))?;
    form.push((
      format!("attached_media[{i}]"),
      json!({"media_fbid": id}).to_string(),
    ));
  }
  graph.post(&format!("{page}/feed"), &form).await
}

/// A Page post's new text: `POST /{page-post-id}` with `message`, as its
/// Page; the photos, link and other settings stay. The Graph API changes
/// only posts the token's own app published.
pub async fn edit(api: &Api, arg: &str, edit: &Edit) -> Result<Action> {
  let Some(text) = edit.text.as_deref() else {
    return Err(Error::input("give the new text"));
  };
  let id = graph_id(arg)?;
  if refs::owner_of(&id).is_none() {
    return Err(Error::input(format!(
      "`{id}` is not a Page post id (PAGE_POST, as `page-posts` lists them)"
    )));
  }
  let graph = api.graph_for(&id).await?;
  let post = graph
    .get(
      &id,
      &fields("id,message,permalink_url,attachments{media_type}"),
    )
    .await?;
  let mut change = Change::new(format!("Facebook post {id}:"));
  change.field("text", &post.str("message").unwrap_or_default(), text);
  let attachments = post.list("attachments.data").len();
  if attachments > 0 {
    change.row("media", &format!("{attachments} (kept)"));
  }
  change.confirm(edit.yes, "change this post on Facebook")?;
  let form = vec![("message".to_owned(), text.to_owned())];
  graph.post(&id, &form).await.map_err(other_app)?;
  let url = post
    .str("permalink_url")
    .unwrap_or_else(|| format!("https://www.facebook.com/{id}"));
  Ok(Action::done("edit", &id).with_id(&id).with_url(url))
}

/// A refused update, with the likeliest reason.
fn other_app(e: Error) -> Error {
  if !matches!(
    e.code,
    ErrorCode::PermissionDenied | ErrorCode::InvalidInput
  ) {
    return e;
  }
  e.with_hint(
    "the Graph API lets an app edit only the Page posts it published: posts made with this \
     token's app (such as `atp facebook post`) can change, posts written on facebook.com or \
     with other tools cannot; the token also needs pages_manage_posts",
  )
}

pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  let id = graph_id(arg)?;
  api.graph_for(&id).await?.delete(&id).await?;
  Ok(Action::done("delete", &id))
}

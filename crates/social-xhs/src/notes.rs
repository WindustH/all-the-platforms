//! Notes: one note's detail, comments and replies
//! (`xhs_cli/client_mixins.py::ReadingEndpointsMixin`), the notes of a
//! listing, your own notes in the creator center, publishing and deleting.

use std::time::Duration;

use social_core::{
  Action, Comment, Draft, Error, Page, PageReq, Post, Result, Value, ValueExt, json,
};

use crate::api::Api;
use crate::events::Association;
use crate::refs::{self, NoteRef, note_url};
use crate::{pages, parse, search, uploads};

const FEED: &str = "/api/sns/web/v1/feed";

const COMMENTS: &str = "/api/sns/web/v2/comment/page";

const SUB_COMMENTS: &str = "/api/sns/web/v2/comment/sub/page";

/// Note detail: the feed API when logged in and a token is known, else (or
/// when it fails) the server-rendered page, which also serves visitors.
pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let mut note = refs::note_ref(api, arg).await?;
  let mut api_error = None;
  let logged_in = api.ctx.http.has_cookie("web_session");
  if let Some(token) = note.token.clone().filter(|_| logged_in) {
    match detail(api, &note, &token).await {
      Ok(post) => return Ok(post),
      Err(e) => {
        tracing::debug!("feed API failed ({e}), falling back to the note page");
        if note.cached {
          refs::forget(&api.ctx, &note.id);
          note.token = None;
        }
        api_error = Some(e);
      }
    }
  }
  let from_page = async {
    let html = pages::html(api, &note).await?;
    let card = pages::note(&html, &note.id)?;
    let item = json!({"id": note.id, "xsec_token": note.token, "note_card": card});
    parse::note(&item, note.source()).ok_or_else(|| Error::not_found(format!("note {}", note.id)))
  };
  match from_page.await {
    Ok(post) => Ok(post),
    Err(e) => Err(api_error.unwrap_or(e)),
  }
}

async fn detail(api: &Api, target: &NoteRef, token: &str) -> Result<Post> {
  let body = json!({
    "source_note_id": target.id,
    "image_formats": ["jpg", "webp", "avif"],
    "extra": {"need_body_topic": "1"},
    "xsec_source": target.source(),
    "xsec_token": token,
  });
  let data = api.post(FEED, &body).await?;
  data
    .list("items")
    .first()
    .and_then(|item| parse::note(item, target.source()))
    .ok_or_else(|| Error::not_found(format!("note {} not found", target.id)))
}

/// A token for `r`: given, cached, or scraped from the note page.
async fn token_for(api: &Api, note: &NoteRef) -> Result<String> {
  if let Some(t) = &note.token {
    return Ok(t.clone());
  }
  let html = pages::html(api, note).await?;
  let (token, source) = pages::token(&html, &note.id).ok_or_else(|| {
    Error::input(format!("no xsec_token for note {}", note.id))
      .with_hint("pass the full note URL from a listing (search, feed, user-posts ...)")
  })?;
  let source = source.unwrap_or_else(|| note.source().to_owned());
  refs::remember(
    &api.ctx,
    [(note.id.as_str(), token.as_str(), source.as_str())],
  );
  Ok(token)
}

pub async fn comments(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Comment>> {
  api.require_login()?;
  let mut note = refs::note_ref(api, arg).await?;
  let cursor = page.cursor.clone().unwrap_or_default();
  let token = token_for(api, &note).await?;
  let data = match comment_page(api, &note.id, &cursor, &token).await {
    // A cached token may have expired: scrape a fresh one once.
    Err(_) if note.cached => {
      refs::forget(&api.ctx, &note.id);
      note.token = None;
      let token = token_for(api, &note).await?;
      comment_page(api, &note.id, &cursor, &token).await?
    }
    other => other?,
  };
  Ok(comment_list(&data))
}

async fn comment_page(api: &Api, id: &str, cursor: &str, token: &str) -> Result<Value> {
  let params = [
    ("note_id", id),
    ("cursor", cursor),
    ("top_comment_id", ""),
    ("image_formats", "jpg,webp,avif"),
    ("xsec_token", token),
  ];
  api.get(COMMENTS, &params).await
}

/// One page of the replies under a root comment, with the web app's
/// parameters; the first page (empty cursor) starts at the oldest reply.
pub async fn replies(api: &Api, arg: &str, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
  api.require_login()?;
  let note = refs::note_ref(api, arg).await?;
  let token = token_for(api, &note).await?;
  let num = page.size_within(30).to_string();
  let cursor = page.cursor.clone().unwrap_or_default();
  let params = [
    ("note_id", note.id.as_str()),
    ("root_comment_id", comment),
    ("num", num.as_str()),
    ("cursor", cursor.as_str()),
    ("image_formats", "jpg,webp,avif"),
    ("top_comment_id", ""),
    ("xsec_token", token.as_str()),
  ];
  Ok(comment_list(&api.get(SUB_COMMENTS, &params).await?))
}

fn comment_list(data: &Value) -> Page<Comment> {
  let items = data.list("comments").iter().map(parse::comment).collect();
  let next = data
    .str("cursor")
    .filter(|_| data.bool("has_more") == Some(true));
  Page::new(items, next)
}

/// Notes of a listing (`items[].note_card`), remembering their tokens.
pub fn listed(api: &Api, data: &Value, source: &str) -> Vec<Post> {
  let items: Vec<&Value> = data
    .list("items")
    .iter()
    .filter(|i| i.at("note_card").is_object())
    .collect();
  let tokens: Vec<(String, String)> = items
    .iter()
    .filter_map(|i| Some((i.str("id")?, i.str("xsec_token")?)))
    .collect();
  refs::remember(
    &api.ctx,
    tokens
      .iter()
      .map(|(id, t)| (id.as_str(), t.as_str(), source)),
  );
  items
    .into_iter()
    .filter_map(|i| parse::note(i, source))
    .collect()
}

// ── your notes ──────────────────────────────────────────────────────────

/// One page (0-based) of the creator center note list.
pub async fn my_notes(api: &Api, req: &PageReq) -> Result<Page<Post>> {
  api.require_login()?;
  let page = req.number_or(0);
  let page_s = page.to_string();
  let data = api
    .creator_get(
      "/api/galaxy/v2/creator/note/user/posted",
      &[("tab", "0"), ("page", &page_s)],
    )
    .await?;
  let rows = match data.list("notes") {
    [] => data.list("note_list"),
    rows => rows,
  };
  let items: Vec<Post> = rows.iter().filter_map(parse::creator_note).collect();
  // The list reports the next page to ask for (the note manager's `page`), or -1 at the end.
  let next = match data.i64("page") {
    Some(n) if n < 0 => None,
    _ if items.is_empty() => None,
    Some(n) if n as u64 > page => Some(n.to_string()),
    _ => Some((page + 1).to_string()),
  };
  Ok(Page::new(items, next))
}

// ── publishing ──────────────────────────────────────────────────────────

/// Creator-center requests go to edith but come from the creator site.
const CREATOR_PAGE: [(&str, &str); 2] = [
  ("origin", crate::api::CREATOR),
  ("referer", "https://creator.xiaohongshu.com/"),
];

const DELETE: &str = "/web_api/sns/capa/postgw/note/delete";

const PUBLISH: &str = "/web_api/sns/v2/note";

const SOURCE: &str = r#"{"type":"web","ids":"","extraInfo":"{\"subType\":\"official\"}"}"#;
// Python's default `json.dumps` spacing, as the reference sends it.

const BINDS: &str = r#"{"version": 1, "noteId": 0, "noteOrderBind": {}, "notePostTiming": {"postTime": null}, "noteCollectionBind": {"id": ""}}"#;

/// Publish an image note: upload each image, resolve topics, post.
pub async fn publish(api: &Api, draft: &Draft) -> Result<Action> {
  publish_note(api, draft, None).await
}

/// Publish an image note, joined to an activity-center activity when given.
pub async fn publish_note(api: &Api, draft: &Draft, event: Option<&Association>) -> Result<Action> {
  check_draft(draft)?;
  api.require_login()?;
  let mut images = Vec::new();
  for path in &draft.images {
    let id = uploads::image(api, path).await?;
    images.push(json!({"file_id": id, "metadata": {"source": -1}}));
  }
  let fields = Fields::new(draft, event);
  let body = json!({
    "common": {
      "type": "normal",
      "title": draft.title.as_deref().unwrap_or_default(),
      "note_id": "",
      "desc": fields.desc.trim_start(),
      "source": fields.source,
      "business_binds": fields.binds,
      "ats": [],
      "hash_tag": hash_tags(api, draft, event).await?,
      "post_loc": {},
      "privacy_info": {"op_type": 1, "type": 0},
    },
    "image_info": {"images": images},
    "video_info": null,
  });
  api
    .ctx
    .http
    .pause(Duration::from_millis(1000), Duration::from_millis(2500))
    .await;
  let data = api.post_with(PUBLISH, &body, &CREATOR_PAGE).await?;
  let mut action = Action::done("publish", draft.title.as_deref().unwrap_or("note"));
  if let Some(id) = data.first_str(&["id", "note_id"]) {
    action = action
      .with_url(note_url(&id, None, refs::SOURCE_FEED))
      .with_id(id);
  }
  Ok(action)
}

fn check_draft(draft: &Draft) -> Result<()> {
  if draft.images.is_empty() {
    return Err(Error::input(
      "a Xiaohongshu note needs at least one image (--image)",
    ));
  }
  if draft.reply_to.is_some() || draft.quote.is_some() {
    return Err(Error::unsupported("post --reply-to / --quote"));
  }
  Ok(())
}

/// The note's topics: the activity's own first, then `--topic` and `#tags`
/// resolved to the topic search's best match.
async fn hash_tags(api: &Api, draft: &Draft, event: Option<&Association>) -> Result<Vec<Value>> {
  let mut tags: Vec<Value> = event.map(|e| e.topics.clone()).unwrap_or_default();
  for name in topic_names(draft) {
    if tags
      .iter()
      .any(|t| t.str("name").as_deref() == Some(name.as_str()))
    {
      continue;
    }
    if let Some(first) = search::first_topic(api, &name).await? {
      tags.push(json!({
        "id": first.str("id").unwrap_or_default(),
        "name": first.str("name").unwrap_or(name),
        "type": "topic",
      }));
    }
  }
  Ok(tags)
}

/// Text, source and business binds of a note: plain, or joined to an activity.
struct Fields {
  desc: String,
  source: String,
  binds: String,
}

impl Fields {
  fn new(draft: &Draft, event: Option<&Association>) -> Self {
    let Some(e) = event else {
      return Self {
        desc: draft.text.clone(),
        source: SOURCE.to_owned(),
        binds: BINDS.to_owned(),
      };
    };
    // The publish page writes the activity's topics into the text too.
    let desc = e
      .topics
      .iter()
      .filter_map(|t| t.str("name"))
      .fold(draft.text.clone(), |d, n| format!("{d} #{n}[话题]#"));
    let binds = json!({
      "version": 1,
      "noteId": 0,
      "bizType": 0,
      "noteOrderBind": {},
      "notePostTiming": {"postTime": null},
      "noteCollectionBind": {"id": ""},
      "optionRelationList": [e.relation],
    });
    Self {
      desc,
      source: e.source(),
      binds: binds.to_string(),
    }
  }
}

/// `--topic` values plus `#tags` of the text, deduplicated, at most 10.
fn topic_names(draft: &Draft) -> Vec<String> {
  let from_text = draft.text.split_whitespace().filter_map(|w| {
    let tag = w.strip_prefix('#')?.split('#').next()?;
    (!tag.is_empty()).then(|| tag.to_owned())
  });
  let mut names: Vec<String> = Vec::new();
  for name in draft.topics.iter().cloned().chain(from_text) {
    if !names.contains(&name) {
      names.push(name);
    }
  }
  names.truncate(10);
  names
}

pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  api.require_login()?;
  let id = refs::note_ref(api, arg).await?.id;
  // The note manager's DELETE_NOTE call (creator center bundle), sent like publishing.
  api
    .post_with(DELETE, &json!({"note_id": id}), &CREATOR_PAGE)
    .await?;
  Ok(Action::done("delete", &id))
}

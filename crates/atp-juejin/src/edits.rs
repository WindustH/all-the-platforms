//! Changing a published article as the article editor (juejin.cn/editor)
//! does: the article's draft (`article_info.draft_id`) is loaded with
//! `article_draft/detail`, saved with the changes through
//! `article_draft/update`, then published again with `article/publish`,
//! all with the editor's CSRF token. Everything not changed goes back as
//! the draft has it: category, tags, cover, summary, columns, topics.
//! Juejin reviews an updated article before showing the new version.
//! Pins (沸点) cannot be edited on Juejin.

use std::sync::LazyLock;

use atp_core::cli::Change;
use atp_core::{Action, Edit, Error, ErrorCode, Result, Value, ValueExt, json};
use regex::Regex;

use crate::api::{AID, Api, SITE};
use crate::markdown;
use crate::refs::{self, Item};
use crate::users;

/// `edit_type` of drafts written in Markdown (0 is the rich-text editor).
const MARKDOWN: i64 = 10;
/// The `html_content` the Markdown editor saves.
const NO_HTML: &str = "deprecated";
/// The image service whose links the editor saves as bare URIs.
const IMAGE_SERVICE: &str = "/tos-cn-i-73owjymdk6/";
/// The mask the editor puts on its word count (`encrypted_word_count`).
const COUNT_MASK: i64 = 538_942;

static MD_IMAGE: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"!\[.*?]\((https?://[^)\s]+/tos-cn-i-73owjymdk6/[^)]+)\)").unwrap());
static HTML_IMAGE: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r#"<img[^>]+src="(https?://[^"]*/tos-cn-i-73owjymdk6/[^"]*)"[^>]*>"#).unwrap()
});

/// Change one of your articles: its title, its Markdown, its tags.
pub async fn edit(api: &Api, arg: &str, edit: &Edit) -> Result<Action> {
  api.require_login()?;
  let Item::Article(id) = refs::item(arg)? else {
    return Err(Error::unsupported(
      "edit of pins (Juejin edits articles only)",
    ));
  };
  let draft_id = own_draft(api, &id).await?;
  let detail = api
    .editor(
      "/content_api/v1/article_draft/detail",
      &json!({ "draft_id": draft_id }),
    )
    .await?;
  let detail = detail.at("data");
  let mut change = Change::new(format!("Juejin article {id}:"));
  let body = changed(api, detail, edit, &mut change).await?;
  change.row("review", "Juejin reviews the new version before showing it");
  change.confirm(edit.yes, "change this article on Juejin")?;
  api
    .editor_write("/content_api/v1/article_draft/update", &body)
    .await?;
  let publish = publish_body(detail, &body);
  let published = api
    .editor_write("/content_api/v1/article/publish", &publish)
    .await?;
  let article = published.str("data.article_id").unwrap_or(id);
  Ok(
    Action::done("edit", &article)
      .with_id(&article)
      .with_url(format!("{SITE}/post/{article}")),
  )
}

/// The update of the draft with the changes, each added to `change`.
async fn changed(api: &Api, detail: &Value, edit: &Edit, change: &mut Change) -> Result<Value> {
  let draft = detail.at("article_draft");
  let markdown_draft = draft.i64("edit_type") == Some(MARKDOWN);
  if edit.text.is_some() && !markdown_draft {
    return Err(Error::unsupported(
      "edit of the text of articles written in the rich-text editor",
    ));
  }
  let mut body = update_body(detail);
  let title = draft.str("title").unwrap_or_default();
  if let Some(new) = edit.title.as_deref().map(str::trim) {
    if new.is_empty() {
      return Err(Error::input("a Juejin article needs a title"));
    }
    change.field("title", &title, new);
    body["title"] = json!(new);
  }
  if let Some(text) = &edit.text {
    change.field("text", &draft.str("mark_content").unwrap_or_default(), text);
    set_markdown(&mut body, text);
  }
  if let Some(names) = &edit.topics {
    let old: Vec<String> = detail
      .list("tags")
      .iter()
      .filter_map(|t| t.str("tag_name"))
      .collect();
    change.field("tags", &old.join(", "), &names.join(", "));
    body["tag_ids"] = json!(tag_ids(api, names).await?);
  }
  Ok(body)
}

/// The draft id of one of your articles, from its details.
async fn own_draft(api: &Api, id: &str) -> Result<String> {
  let body = json!({
    "article_id": id,
    "forbid_count": true,
    "is_pre_load": true,
    "req_from": 0,
    "client_type": AID,
  });
  let v = api.post("/content_api/v1/article/detail", &body).await?;
  let author = v.str("data.author_user_info.user_id");
  if author.is_none() || author != Some(users::whoami(api).await?.id) {
    return Err(Error::new(
      ErrorCode::PermissionDenied,
      format!("article {id} is not yours; Juejin edits only your own"),
    ));
  }
  v.str("data.article_info.draft_id")
    .filter(|draft| draft != "0")
    .ok_or_else(|| Error::upstream(format!("Juejin named no draft for article {id}")))
}

/// What `article_draft/update` takes, as the editor sends the draft it
/// loaded (`fromDescriptor` of the details): its own fields, the tags and
/// topics of the details, the images on Juejin's storage as bare URIs.
fn update_body(detail: &Value) -> Value {
  let draft = detail.at("article_draft");
  let markdown_draft = draft.i64("edit_type") == Some(MARKDOWN);
  let tags: Vec<String> = match detail.list("tags") {
    [] => strings(draft.list("tag_ids")),
    tags => tags.iter().filter_map(|t| t.str("tag_id")).collect(),
  };
  let category = draft
    .str("category_id")
    .or_else(|| detail.str("category.category_id"))
    .unwrap_or_else(|| "0".into());
  let mut body = json!({
    "id": draft.str("id"),
    "category_id": category,
    "tag_ids": tags,
    "link_url": draft.str("link_url").unwrap_or_default(),
    "cover_image": draft.str("cover_image").unwrap_or_default(),
    "is_gfw": draft.at("is_gfw"),
    "title": draft.str("title").unwrap_or_default(),
    "brief_content": draft.at("brief_content"),
    "is_english": draft.at("is_english"),
    "is_original": draft.at("is_original"),
    "edit_type": if markdown_draft { MARKDOWN } else { 0 },
    "theme_ids": theme_ids(detail),
  });
  if markdown_draft {
    set_markdown(&mut body, &draft.str("mark_content").unwrap_or_default());
  } else {
    let (markdown, pics) = with_uris(&draft.str("mark_content").unwrap_or_default());
    let (html, _) = with_uris(&draft.str("html_content").unwrap_or_default());
    body["mark_content"] = json!(markdown);
    body["html_content"] = json!(html);
    body["pics"] = json!(pics);
  }
  body
}

/// The Markdown text with its images as bare URIs, the editor's
/// `html_content` placeholder, and the images in `pics`.
fn set_markdown(body: &mut Value, text: &str) {
  let (markdown, pics) = with_uris(text);
  body["mark_content"] = json!(markdown);
  body["html_content"] = json!(NO_HTML);
  body["pics"] = json!(pics);
}

/// The text with each image of Juejin's storage (Markdown `![](…)` and
/// `<img src="…">`) linked by its URI (the path before `~`, without the
/// leading `/`), and those images as `pics` entries, as the editor saves
/// them.
fn with_uris(text: &str) -> (String, Vec<Value>) {
  let mut out = text.to_owned();
  let mut pics = Vec::new();
  for pattern in [&*MD_IMAGE, &*HTML_IMAGE] {
    let found: Vec<(String, String)> = pattern
      .captures_iter(&out)
      .filter_map(|c| Some((c.get(0)?.as_str().to_owned(), c.get(1)?.as_str().to_owned())))
      .collect();
    for (whole, link) in found {
      // A Markdown image's link ends at a space (`![](URL "title")`).
      let link = link
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_owned();
      let Some(uri) = uri(&link) else { continue };
      out = out.replace(&whole, &whole.replacen(&link, &uri, 1));
      pics.push(json!({ "pic_url": link, "pic_uri": uri }));
    }
  }
  (out, pics)
}

/// `tos-cn-i-73owjymdk6/abc` of `https://p3-juejin.byteimg.com/tos-cn-i-73owjymdk6/abc~tplv-….image`.
fn uri(link: &str) -> Option<String> {
  let path = url::Url::parse(link).ok()?.path().to_owned();
  let path = path.split('~').next()?.trim_start_matches('/');
  path
    .contains(IMAGE_SERVICE.trim_matches('/'))
    .then(|| path.to_owned())
}

/// The ids of tags named as on Juejin (`query_tag_list`, the editor's tag
/// search); a name without an exact match is refused with the closest ones.
async fn tag_ids(api: &Api, names: &[String]) -> Result<Vec<String>> {
  if names.is_empty() {
    return Err(Error::input("a Juejin article needs at least one tag"));
  }
  let mut ids = Vec::new();
  for name in names {
    let name = name.trim_start_matches('#').trim();
    let body = json!({ "cursor": "0", "key_word": name, "limit": 10, "sort_type": 1 });
    let v = api.editor("/tag_api/v1/query_tag_list", &body).await?;
    let tags: Vec<(String, String)> = v
      .list("data")
      .iter()
      .filter_map(|t| Some((t.str("tag.tag_id")?, t.str("tag.tag_name")?)))
      .collect();
    let Some((id, _)) = tags.iter().find(|(_, n)| n.eq_ignore_ascii_case(name)) else {
      let close: Vec<&str> = tags.iter().map(|(_, n)| n.as_str()).collect();
      return Err(Error::not_found(format!(
        "no Juejin tag named {name} (close: {})",
        if close.is_empty() {
          "none".to_owned()
        } else {
          close.join(", ")
        }
      )));
    };
    ids.push(id.clone());
  }
  Ok(ids)
}

/// What `article/publish` takes: the draft, its columns and topics as the
/// editor keeps them, and the editor's word count with its mask.
fn publish_body(detail: &Value, update: &Value) -> Value {
  let columns: Vec<String> = detail
    .list("columns")
    .iter()
    .filter_map(|c| c.str("column_id"))
    .collect();
  let text = update.str("mark_content").unwrap_or_default();
  let count = markdown::to_text(&text)
    .0
    .chars()
    .filter(|c| !c.is_whitespace())
    .count() as i64;
  json!({
    "draft_id": update.str("id"),
    "sync_to_org": false,
    "column_ids": columns,
    "theme_ids": update.at("theme_ids"),
    "encrypted_word_count": (count ^ COUNT_MASK) + COUNT_MASK,
    "origin_word_count": count,
  })
}

/// The topics (创作话题) of the draft, as the editor keeps them.
fn theme_ids(detail: &Value) -> Vec<String> {
  detail
    .list("theme_list")
    .iter()
    .filter_map(|t| t.str("theme.theme_id"))
    .collect()
}

fn strings(values: &[Value]) -> Vec<String> {
  values
    .iter()
    .filter_map(|v| v.as_str().map(str::to_owned))
    .collect()
}

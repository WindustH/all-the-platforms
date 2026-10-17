//! Changing a published work as the creator platform does it: the edit
//! page (作品管理 → 修改作品) loads the work (`item/info`) and whether it
//! may still change (`update/desc/valid/v2`), then sends the text, title,
//! hashtags and cover through `update/desc/`; the manage page's
//! permission form sets who can see it (`aweme/update/`). Nothing goes out
//! before the person has seen the change and said yes.

use std::time::Duration;

use atp_core::cli::Change;
use atp_core::output::note;
use atp_core::{Action, Edit, Error, ErrorCode, Result, Value, ValueExt};

use super::draft::{IMAGE_TITLE_MAX, TEXT_MAX, VIDEO_TITLE_MAX, utf16};
use super::{MANAGE_PAGE, picture};
use crate::api::Api;
use crate::creator::Creator;
use crate::items::{self, Covers, Editor, Picture, Tag};
use crate::uploads::Uploader;
use crate::{refs, users};

const ITEM_INFO: &str = "/web/api/media/item/info/";
const EDITABLE: &str = "/web/api/media/update/desc/valid/v2/";
const UPDATE_DESC: &str = "/web/api/media/update/desc/";
const UPDATE_PERMISSION: &str = "/web/api/media/aweme/update/";
const EDIT_PAGE: &str = "https://creator.douyin.com/creator-micro/content/post";

/// A published work as the edit page sees it.
struct Work {
  id: String,
  kind: Kind,
  title: String,
  editor: Editor,
  /// 0 public, 1 only me, 2 friends (`status.private_status`).
  visibility: i64,
  download: i64,
}

enum Kind {
  Video { vid: String, covers: Covers },
  Images,
}

/// Change one of the account's works.
pub async fn edit(api: &Api, post: &str, edit: &Edit) -> Result<Action> {
  let id = refs::aweme(api, post).await?;
  let creator = Creator::open(api).await?;
  let info = creator.get(ITEM_INFO).arg("item_id", &id).send().await?;
  let work = Work::of(&id, info.at("aweme"))?;
  let words = edit.text.is_some() || edit.title.is_some() || edit.topics.is_some();
  let describes = words || edit.cover.is_some();
  let visibility = edit.visibility.as_deref().map(visibility_code);
  let edits = if describes {
    Some(editable(&creator, &id).await?)
  } else {
    None
  };
  let (title, editor) = change(&work, edit)?;
  ask(&work, edit, &title, &editor, edits.as_deref())?;

  if describes {
    let cover = match &edit.cover {
      Some(path) => {
        let uploader = Uploader::new(&creator, &users::uid(api).await?).await?;
        note("uploading the cover");
        Some(picture(uploader.image(path).await?))
      }
      None => None,
    };
    let form = match &work.kind {
      Kind::Video { vid, covers } => {
        items::video_modify(&id, vid, &title, &editor, covers, cover.as_ref())
      }
      Kind::Images => {
        let words = words.then_some((title.as_str(), &editor));
        items::images_modify(&id, words, cover.as_ref())
      }
    };
    pause(api).await;
    creator
      .post_form(UPDATE_DESC, &form)
      .arg("read_aid", 2906)
      .referer(&work.page())
      .send()
      .await?;
  }
  if let Some(code) = visibility.filter(|code| *code != work.visibility) {
    pause(api).await;
    let form = [
      ("item_id", id.clone()),
      ("download", work.download.to_string()),
      ("visibility_type", code.to_string()),
    ]
    .map(|(k, v)| (k.to_owned(), v));
    creator
      .post_form(UPDATE_PERMISSION, &form)
      .referer(MANAGE_PAGE)
      .send()
      .await?;
  }
  Ok(Action::done("edit", &id).with_id(&id).with_url(work.url()))
}

impl Work {
  fn of(id: &str, aweme: &Value) -> Result<Self> {
    if aweme.is_null() {
      return Err(Error::not_found(format!(
        "no work {id} on this account's creator platform"
      )));
    }
    let images = !aweme.list("images").is_empty();
    let vid = first(aweme, &["video.vid", "vid", "video.play_addr.uri"]);
    let (kind, (title, editor)) = match vid {
      _ if images => (Kind::Images, items::image_editor(aweme)?),
      Some(vid) => {
        let covers = covers(aweme);
        tracing::debug!("item/info: vid {vid}, covers {covers:?}");
        (Kind::Video { vid, covers }, items::video_editor(aweme)?)
      }
      None => {
        tracing::debug!("item/info aweme keys: {:?}", keys(aweme));
        return Err(Error::unsupported(
          "edit of this kind of work (only videos and image posts)",
        ));
      }
    };
    Ok(Self {
      id: id.to_owned(),
      kind,
      title,
      editor,
      visibility: aweme.i64("status.private_status").unwrap_or(0),
      download: aweme.i64("video_control.allow_download").unwrap_or(1),
    })
  }

  fn name(&self) -> &'static str {
    match self.kind {
      Kind::Video { .. } => "video",
      Kind::Images => "image post",
    }
  }

  fn url(&self) -> String {
    match self.kind {
      Kind::Video { .. } => refs::video_url(&self.id),
      Kind::Images => refs::note_url(&self.id),
    }
  }

  /// The edit page: the referer of `update/desc/`.
  fn page(&self) -> String {
    let form = match self.kind {
      Kind::Video { .. } => "video",
      Kind::Images => "image",
    };
    format!("{EDIT_PAGE}/{form}?enter_from=manage&mid={}", self.id)
  }
}

/// The title and editor content after the edit: a new text replaces the
/// body, new topics the hashtags; what is not given stays as it was, and
/// an untouched text keeps its hashtags where they are.
fn change(work: &Work, edit: &Edit) -> Result<(String, Editor)> {
  let title = edit
    .title
    .as_deref()
    .map_or_else(|| work.title.clone(), |t| t.trim().to_owned());
  let editor = if edit.text.is_none() && edit.topics.is_none() {
    work.editor.clone()
  } else {
    let old = work.editor.tags();
    let tags: Vec<Tag> = match &edit.topics {
      Some(names) => names
        .iter()
        .map(|t| t.trim().trim_start_matches('#').trim().to_owned())
        .filter(|name| !name.is_empty())
        .map(|name| {
          let id = old.iter().find(|t| t.name == name).map_or(0, |t| t.id);
          Tag { name, id }
        })
        .collect(),
      None => old,
    };
    let body = edit.text.clone().unwrap_or_else(|| work.editor.body());
    Editor::typed(&body, &tags)
  };
  let max = match work.kind {
    Kind::Video { .. } => VIDEO_TITLE_MAX,
    Kind::Images => IMAGE_TITLE_MAX,
  };
  if utf16(&title) > max {
    return Err(Error::input(format!(
      "a Douyin {}'s title takes at most {max} characters",
      work.name()
    )));
  }
  if utf16(&editor.text) > TEXT_MAX {
    return Err(Error::input("the text takes at most 1000 characters"));
  }
  Ok((title, editor))
}

/// Whether the work's text, title and cover may still change, and how
/// many edits are used (`basic_fields`).
async fn editable(creator: &Creator<'_>, id: &str) -> Result<String> {
  let v = creator
    .get(EDITABLE)
    .arg("item_id", id)
    .arg("action", "init")
    .send()
    .await?;
  let fields = v.at("basic_fields");
  if fields.bool("can_edit") == Some(false) {
    let reason = fields
      .str("toast")
      .unwrap_or_else(|| "this work can no longer be edited".into());
    return Err(Error::new(
      ErrorCode::PermissionDenied,
      format!("Douyin: {reason}"),
    ));
  }
  Ok(
    match (fields.u64("edited_times"), fields.u64("max_edit_times")) {
      (Some(used), Some(max)) if max > 0 => format!("{used} of {max} used"),
      _ => String::new(),
    },
  )
}

/// Show the change and ask (or take `--yes`).
fn ask(work: &Work, edit: &Edit, title: &str, editor: &Editor, edits: Option<&str>) -> Result<()> {
  let hashtags = |e: &Editor| {
    let names: Vec<String> = e.tags().iter().map(|t| format!("#{}", t.name)).collect();
    names.join(" ")
  };
  let visibility = edit
    .visibility
    .as_deref()
    .map_or(work.visibility, visibility_code);
  let mut change = Change::new(format!("Douyin {} {}:", work.name(), work.id));
  change
    .field("title", &work.title, title)
    .field("text", &work.editor.body(), &editor.body())
    .field("hashtags", &hashtags(&work.editor), &hashtags(editor))
    .field(
      "visibility",
      visibility_name(work.visibility),
      visibility_name(visibility),
    );
  if let Some(path) = &edit.cover {
    change.row("cover", &path.display().to_string());
  }
  if let Some(edits) = edits.filter(|e| !e.is_empty()) {
    change.row("edits", edits);
  }
  change.confirm(edit.yes, "change this work on Douyin")
}

/// A video's covers as the edit page fills them in: `Cover` (or `cover`),
/// with the video's size when the cover has none, and `horizontal_cover`.
fn covers(aweme: &Value) -> Covers {
  let pick = |paths: &[&str]| first(aweme, paths).unwrap_or_default();
  let size = |key: &str, fallback: &str| {
    aweme
      .u64(&format!("Cover.{key}"))
      .or_else(|| aweme.u64(&format!("cover.{key}")))
      .or_else(|| aweme.u64(fallback))
      .unwrap_or(0)
  };
  let cover = Picture {
    uri: pick(&["Cover.uri", "cover.uri", "video.cover.uri"]),
    width: size("width", "video.width"),
    height: size("height", "video.height"),
  };
  let horizontal = aweme.str("horizontal_cover.uri").map(|uri| Picture {
    uri,
    width: aweme.u64("horizontal_cover.width").unwrap_or(0),
    height: aweme.u64("horizontal_cover.height").unwrap_or(0),
  });
  Covers { cover, horizontal }
}

fn visibility_code(name: &str) -> i64 {
  match name {
    "private" => 1,
    "friends" => 2,
    _ => 0,
  }
}

fn visibility_name(code: i64) -> &'static str {
  match code {
    1 => "private",
    2 => "friends",
    _ => "public",
  }
}

fn first(v: &Value, paths: &[&str]) -> Option<String> {
  paths.iter().find_map(|p| v.str(p))
}

fn keys(v: &Value) -> Vec<&str> {
  v.as_object()
    .map(|o| o.keys().map(String::as_str).collect())
    .unwrap_or_default()
}

async fn pause(api: &Api) {
  api
    .ctx
    .http
    .pause(Duration::from_secs(2), Duration::from_secs(6))
    .await;
}

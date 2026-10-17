//! The `item` of `create_v2`, built the way the creator page's publish
//! form merges what each of its fields contributes (`beforePost` of the
//! fields of the video, image-post and article forms). Pure functions.
//!
//! Text offsets follow the page: UTF-16 units for videos and image posts
//! (JavaScript string lengths), code points for article topics.

mod text;

use atp_core::{Value, json};

pub use self::text::{Editor, image_editor, video_editor};
use self::text::{Text, article_extra, image_fields, image_text, video_fields, video_text};

/// `media_type` of each format.
pub const VIDEO: i64 = 4;
pub const IMAGES: i64 = 2;
pub const ARTICLE: i64 = 43;
/// The choice of the 「内容由AI生成」 declaration (自主声明).
pub const AI_DECLARATION: &str = "aigc";

/// What every format shares.
#[derive(Debug, Clone, Default)]
pub struct Settings {
  pub creation_id: String,
  /// 0 public, 1 only me, 2 friends.
  pub visibility: i64,
  /// Unix seconds of a scheduled post, 0 for now.
  pub timing: i64,
  pub ai: bool,
  /// A collection and the episode index the post takes in it.
  pub mix: Option<(String, i64)>,
  /// A music and its length in milliseconds.
  pub music: Option<(String, i64)>,
}

/// A hashtag with the id Douyin knows it by (0 when typed, not picked).
#[derive(Debug, Clone)]
pub struct Tag {
  pub name: String,
  pub id: u64,
}

/// The words of a post: a title, the text (or an article's Markdown) and
/// its hashtags.
#[derive(Debug, Clone, Copy)]
pub struct Words<'a> {
  pub title: &'a str,
  pub body: &'a str,
  pub tags: &'a [Tag],
}

/// An uploaded image.
#[derive(Debug, Clone, Default)]
pub struct Picture {
  pub uri: String,
  pub width: u64,
  pub height: u64,
}

/// A video item. `cover` is an uploaded cover image, else the first
/// frame (`poster`) stands.
pub fn video(
  s: &Settings,
  words: Words,
  vid: &str,
  poster: &str,
  cover: Option<&Picture>,
) -> Value {
  let text = video_text(words);
  let common = json!({
    "text": text.text,
    "caption": text.caption,
    "item_title": text.title,
    "activity": "[]",
    "text_extra": text.extra.to_string(),
    "challenges": text.challenges.to_string(),
    "mentions": "[]",
    "hashtag_source": text.source,
    "hot_sentence": "",
    "interaction_stickers": "[]",
    "visibility_type": s.visibility,
    "download": 1,
    "timing": s.timing,
    "creation_id": s.creation_id,
    "media_type": VIDEO,
    "video_id": vid,
    "music_source": 0,
    "music_id": null,
  });
  let mut item = json!({
    "common": common,
    "cover": video_cover(poster, cover),
    "mix": mix(s, false),
    "chapter": { "chapter": chapter(s) },
    "anchor": {},
    "sync": { "should_sync": false, "sync_to_toutiao": 0 },
    "open_platform": {},
    "assistant": { "is_preview": 0, "is_post_assistant": 1 },
    "selected_member": { "is_selected_member_video": false },
  });
  declare(&mut item, s);
  json!({ "item": item })
}

/// An image post: the title before the text as the page joins them.
pub fn images(s: &Settings, words: Words, pictures: &[Picture]) -> Value {
  let text = image_text(words);
  let list: Vec<Value> = pictures
    .iter()
    .map(|p| json!({ "uri": p.uri, "width": p.width, "height": p.height }))
    .collect();
  let mut common = json!({
    "text": text.text,
    "text_extra": text.extra.to_string(),
    "activity": "[]",
    "challenges": text.challenges.to_string(),
    "hashtag_source": text.source,
    "mentions": "[]",
    "media_type": IMAGES,
    "images": list,
    "visibility_type": s.visibility,
    "download": 1,
    "creation_id": s.creation_id,
  });
  extras(&mut common, s);
  let poster = pictures.first().map(|p| p.uri.as_str()).unwrap_or_default();
  let mut item = json!({ "common": common, "cover": { "poster": poster }, "anchor": {} });
  if s.mix.is_some() {
    item["mix"] = mix(s, true);
  }
  declare(&mut item, s);
  json!({ "item": item })
}

/// An article: Markdown body, title with its topics, abstract and cover.
pub fn article(s: &Settings, words: Words, summary: &str, cover: &Picture) -> Value {
  let (title, markdown, tags) = (words.title.trim(), words.body, words.tags);
  let mut text = title.to_owned();
  if !tags.is_empty() {
    let names: Vec<String> = tags.iter().map(|t| format!("#{}", t.name)).collect();
    text = format!("{text} {}", names.join(" "));
  }
  let mut common = json!({
    "text": text,
    "description": summary.trim(),
    "long_article": markdown.trim(),
    "long_article_image_info": [],
    "text_extra": article_extra(title, tags).to_string(),
    "media_type": ARTICLE,
    "long_article_version": "1",
    "visibility_type": s.visibility,
    "creation_id": s.creation_id,
  });
  extras(&mut common, s);
  let cover = json!({
    "custom_cover_image_width": cover.width,
    "custom_cover_image_height": cover.height,
    "upload_poster": cover.uri,
  });
  json!({ "item": { "common": common, "cover": cover } })
}

/// A form of `update/desc/`, as `qs.stringify` sends it.
pub type Form = Vec<(String, String)>;

/// The covers of a published video: its cover and, when it has one, its
/// horizontal cover.
#[derive(Debug, Clone, Default)]
pub struct Covers {
  pub cover: Picture,
  pub horizontal: Option<Picture>,
}

/// The `update/desc/` form of a video (the edit page's `beforeModify`):
/// the item, its text as the editor holds it, and its covers, `new` in
/// place of the cover when one was uploaded.
pub fn video_modify(
  item_id: &str,
  vid: &str,
  title: &str,
  editor: &Editor,
  covers: &Covers,
  new: Option<&Picture>,
) -> Form {
  let text = video_fields(title, editor);
  let mut form = modify_head(item_id);
  form.push(("video_id".into(), vid.to_owned()));
  push_text(&mut form, &text, true);
  let cover = new.unwrap_or(&covers.cover);
  form.push(("croppedUri".into(), cover.uri.clone()));
  push_size(&mut form, "custom_cover_image", cover);
  // The page keeps a landscape cover as the horizontal one too.
  let old = &covers.cover;
  let horizontal = match (&covers.horizontal, new) {
    (_, Some(new)) if new.width > new.height => Some(new),
    (Some(h), _) => Some(h),
    (None, _) if old.width > old.height => Some(old),
    _ => None,
  };
  if let Some(h) = horizontal {
    form.push(("horizontal_custom_cover_image_uri".into(), h.uri.clone()));
    push_size(&mut form, "horizontal_custom_cover_image", h);
  }
  form
}

/// The `update/desc/` form of an image post: the text when it changed and
/// the cover when one was uploaded.
pub fn images_modify(
  item_id: &str,
  words: Option<(&str, &Editor)>,
  cover: Option<&Picture>,
) -> Form {
  let mut form = modify_head(item_id);
  if let Some((title, editor)) = words {
    push_text(&mut form, &image_fields(title, editor), false);
  }
  if let Some(cover) = cover {
    form.push(("croppedUri".into(), cover.uri.clone()));
    push_size(&mut form, "custom_cover_image", cover);
  }
  form
}

fn modify_head(item_id: &str) -> Form {
  vec![
    ("item_id".into(), item_id.to_owned()),
    ("is_new_update".into(), "1".into()),
  ]
}

/// The text fields of `update/desc/`; a video's carry its caption and title.
fn push_text(form: &mut Form, text: &Text, video: bool) {
  form.push(("text".into(), text.text.clone()));
  if video {
    form.push(("caption".into(), text.caption.clone()));
    form.push(("item_title".into(), text.title.clone()));
  }
  form.push(("activity".into(), "[]".into()));
  form.push(("text_extra".into(), text.extra.to_string()));
  form.push(("challenges".into(), text.challenges.to_string()));
  form.push(("mentions".into(), "[]".into()));
  if video {
    form.push(("title_permission".into(), "1".into()));
  }
}

fn push_size(form: &mut Form, prefix: &str, picture: &Picture) {
  if picture.width > 0 && picture.height > 0 {
    form.push((format!("{prefix}_width"), picture.width.to_string()));
    form.push((format!("{prefix}_height"), picture.height.to_string()));
  }
}

/// `timing` when scheduled and the music of image posts and articles.
fn extras(common: &mut Value, s: &Settings) {
  if s.timing > 0 {
    common["timing"] = json!(s.timing);
  }
  if let Some((id, millis)) = &s.music {
    common["music_id"] = json!(id);
    common["music_end_time"] = json!(millis);
  }
}

/// The 「内容由AI生成」 declaration: `declare.user_declare_info`.
fn declare(item: &mut Value, s: &Settings) {
  if s.ai {
    let info = json!({ "choose_value": AI_DECLARATION }).to_string();
    item["declare"] = json!({ "user_declare_info": info });
  }
}

/// `mix`: empty without a collection. Image posts count a scheduled post
/// one episode further, as the page does.
fn mix(s: &Settings, images: bool) -> Value {
  match &s.mix {
    Some((id, order)) => {
      let order = if images && s.timing > 0 {
        order + 1
      } else {
        *order
      };
      json!({ "mix_id": id, "mix_order": order })
    }
    None => json!({}),
  }
}

/// The chapters field of a video without chapters.
fn chapter(s: &Settings) -> String {
  let now = if s.timing > 0 {
    s.timing
  } else {
    jiff::Timestamp::now().as_second()
  };
  json!({
    "chapter_abstract": "",
    "chapter_details": [],
    "chapter_type": 1,
    "chapter_tools_info": {
      "chapter_recommend_detail": [],
      "chapter_recommend_abstract": "",
      "chapter_source": 2,
      "chapter_recommend_type": -2,
      "create_date": now,
      "is_pc": "1",
      "is_pre_generated": "0",
      "is_syn": "1",
    },
  })
  .to_string()
}

/// A video's cover: the first frame, or an uploaded image (`upload_poster`).
fn video_cover(poster: &str, cover: Option<&Picture>) -> Value {
  let Some(picture) = cover else {
    let info = json!({ "firstFrameCoverUri": poster, "videoName": "", "uri": poster, "url": "", "posterDelay": 0 });
    return json!({
      "cover_text_uri": null,
      "cover_text": null,
      "poster": poster,
      "poster_delay": 0,
      "cover_tools_extend_info": cover_state(info, Value::Null).to_string(),
      "cover_tools_info": "{}",
    });
  };
  let uploaded = json!({
    "uri": picture.uri,
    "url": "",
    "customCoverImageWidth": picture.width,
    "customCoverImageHeight": picture.height,
  });
  let mut info = uploaded.clone();
  info["posterDelay"] = json!(0);
  info["edited"] = json!(true);
  info["type"] = json!(1);
  json!({
    "cover_text_uri": null,
    "cover_text": null,
    "custom_cover_image_height": picture.height,
    "custom_cover_image_width": picture.width,
    "upload_poster": picture.uri,
    "cover_tools_extend_info": cover_state(info, uploaded).to_string(),
    "cover_tools_info": "{}",
  })
}

/// The cover editor's state (`cover_tools_extend_info`), untouched but for
/// the cover and an uploaded background.
fn cover_state(cover: Value, background: Value) -> Value {
  json!({
    "recommendServerInfo": { "res": [], "times": [] },
    "recommendCoverList": [],
    "recommendCoverInfo": {
      "isFromRecommend": false,
      "isDefaultSelect": false,
      "isRecommendClickFrom": "",
      "selectInfo": {},
      "editingInfo": {},
    },
    "recommendCoverTime": 0,
    "coverInfo": cover,
    "coverUrl": "",
    "coverHorizontalInfo": null,
    "coverHorizontalUrl": "",
    "pasterInfo": null,
    "stateInfo": null,
    "croppedCoverInfo": null,
    "uploadBackgroundInfo": background,
    "uploadPasterInfo": null,
    "uploadCoverStateInfo": null,
    "xiguaCoverInfo": { "posterDelay": 0 },
    "xiguaPasterInfo": null,
    "xiguaStateInfo": null,
    "xiguaUploadCoverStateInfo": null,
    "xiguaUploadBackgroundInfo": null,
    "xiguaUploadPasterInfo": null,
    "editXigua": false,
    "coverSource": "",
    "previewVideoList": [],
  })
}

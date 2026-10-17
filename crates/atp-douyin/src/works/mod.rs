//! Publishing and deleting works (作品) as the creator platform's web
//! uploader does: videos, image posts (图文) and articles (文章), their
//! files sent through [`uploads`](crate::uploads), then `create_v2`.
//! Nothing goes out before the person has seen a summary and said yes.

mod draft;

use std::path::Path;
use std::time::Duration;

use atp_core::output::note;
use atp_core::{Action, Draft, Error, Result, Value, ValueExt};
use rand::Rng;

use self::draft::{Kind, article_body, ask, check};

use crate::api::Api;
use crate::creator::Creator;
use crate::items::{self, Picture, Settings, Tag, Words};
use crate::sign::guard::Guard;
use crate::uploads::{self, Uploader};
use crate::{library, refs, users};

const CREATE: &str = "/web/api/media/aweme/create_v2/";
const DELETE: &str = "/web/api/media/aweme/delete/";
const TAG_SUGGEST: &str = "/aweme/v1/search/challengesug/";
const MUSIC_DETAIL: &str = "/aweme/v1/web/music/detail/";
const POST_PAGE: &str = "https://creator.douyin.com/creator-micro/content/post";
const MANAGE_PAGE: &str = "https://creator.douyin.com/creator-micro/content/manage";

/// Publish a draft: check it, ask, upload its files, create the work.
pub async fn publish(api: &Api, draft: &Draft) -> Result<Action> {
  let kind = Kind::of(draft)?;
  check(draft, &kind)?;
  let markdown = match kind {
    Kind::Article(path) => article_body(path)?,
    _ => String::new(),
  };
  ask(draft, &kind)?;
  // Fail before uploading when the session cannot sign the final call.
  api.require_login()?;
  Guard::load(&api.ctx)?;
  let creator = Creator::open(api).await?;
  let settings = settings(&creator, draft).await?;
  let tags = tags(&creator, draft, &kind).await?;
  let uploader = Uploader::new(&creator, &users::uid(api).await?).await?;
  let words = Words {
    title: draft.title.as_deref().unwrap_or_default(),
    body: &draft.text,
    tags: &tags,
  };
  let body = match kind {
    Kind::Video(path) => video(&uploader, draft, &settings, words, path).await?,
    Kind::Images => images(&uploader, draft, &settings, words).await?,
    Kind::Article(_) => article(&uploader, draft, &settings, &markdown, &tags).await?,
  };
  api
    .ctx
    .http
    .pause(Duration::from_secs(2), Duration::from_secs(6))
    .await;
  let v = creator
    .post_json(CREATE, &body)
    .arg("read_aid", 2906)
    .referer(&kind.page())
    .send()
    .await?;
  created(&v, &kind)
}

/// Delete one of the account's works.
pub async fn delete(api: &Api, post: &str) -> Result<Action> {
  let id = refs::aweme(api, post).await?;
  let creator = Creator::open(api).await?;
  api
    .ctx
    .http
    .pause(Duration::from_secs(2), Duration::from_secs(6))
    .await;
  creator
    .post_form(DELETE, &[("item_id".into(), id.clone())])
    .referer(MANAGE_PAGE)
    .send()
    .await?;
  Ok(Action::done("delete", &id))
}

async fn video(
  up: &Uploader<'_>,
  draft: &Draft,
  s: &Settings,
  words: Words<'_>,
  path: &Path,
) -> Result<Value> {
  note("uploading the video");
  let video = up.video(path).await?;
  let cover = match &draft.cover {
    Some(cover) => Some(picture(up.image(cover).await?)),
    None => None,
  };
  if video.poster.is_empty() && cover.is_none() {
    return Err(Error::upstream(
      "Douyin made no first-frame cover; give one with --cover",
    ));
  }
  Ok(items::video(
    s,
    words,
    &video.vid,
    &video.poster,
    cover.as_ref(),
  ))
}

async fn images(up: &Uploader<'_>, draft: &Draft, s: &Settings, words: Words<'_>) -> Result<Value> {
  let mut pictures = Vec::new();
  for (i, path) in draft.images.iter().enumerate() {
    note(&format!(
      "uploading image {} of {}",
      i + 1,
      draft.images.len()
    ));
    pictures.push(picture(up.image(path).await?));
  }
  Ok(items::images(s, words, &pictures))
}

async fn article(
  up: &Uploader<'_>,
  draft: &Draft,
  s: &Settings,
  markdown: &str,
  tags: &[Tag],
) -> Result<Value> {
  let path = draft
    .cover
    .as_deref()
    .ok_or_else(|| Error::input("an article needs a cover (--cover)"))?;
  note("uploading the cover");
  let cover = picture(up.image(path).await?);
  let words = Words {
    title: draft.title.as_deref().unwrap_or_default(),
    body: markdown,
    tags,
  };
  Ok(items::article(s, words, &draft.text, &cover))
}

fn picture(image: uploads::Image) -> Picture {
  Picture {
    uri: image.uri,
    width: image.width,
    height: image.height,
  }
}

/// The new work's id and link.
fn created(v: &Value, kind: &Kind) -> Result<Action> {
  let id = v
    .first_str(&["item_id", "aweme_id", "aweme.aweme_id"])
    .ok_or_else(|| {
      Error::upstream(format!(
        "Douyin accepted the post but named no id: {}",
        atp_core::text::truncate(&v.to_string(), 300)
      ))
    })?;
  let action = Action::done("post", &id).with_id(&id);
  Ok(match kind {
    Kind::Video(_) => action.with_url(refs::video_url(&id)),
    Kind::Images => action.with_url(refs::note_url(&id)),
    Kind::Article(_) => action,
  })
}

async fn settings(creator: &Creator<'_>, draft: &Draft) -> Result<Settings> {
  let mix = match &draft.collection {
    Some(arg) => Some(library::mix_slot(creator, arg).await?),
    None => None,
  };
  let music = match &draft.music {
    Some(id) => Some((
      id.trim().to_owned(),
      music_length(creator.api, id.trim()).await?,
    )),
    None => None,
  };
  Ok(Settings {
    creation_id: creation_id(),
    visibility: match draft.visibility.as_deref() {
      Some("private") => 1,
      Some("friends") => 2,
      _ => 0,
    },
    timing: draft.schedule.map_or(0, |t| t.as_second()),
    ai: draft.ai_generated,
    mix,
    music,
  })
}

/// Hashtags: typed ones for videos and image posts (id 0, as when typed
/// in the editor); an article's topics must be ones Douyin suggests.
async fn tags(creator: &Creator<'_>, draft: &Draft, kind: &Kind<'_>) -> Result<Vec<Tag>> {
  let names = draft
    .topics
    .iter()
    .map(|t| t.trim().trim_start_matches('#').trim().to_owned())
    .filter(|t| !t.is_empty());
  let mut tags = Vec::new();
  for name in names {
    let id = match kind {
      Kind::Article(_) => topic_id(creator, &name).await?,
      _ => 0,
    };
    tags.push(Tag { name, id });
  }
  Ok(tags)
}

/// The id of the topic Douyin suggests under exactly this name.
async fn topic_id(creator: &Creator<'_>, name: &str) -> Result<u64> {
  let v = creator
    .get(TAG_SUGGEST)
    .arg("source", "challenge_create")
    .arg("aid", "2906")
    .arg("keyword", name)
    .send()
    .await?;
  v.list("sug_list")
    .iter()
    .find(|s| s.str("cha_name").as_deref() == Some(name))
    .and_then(|s| s.u64("cid"))
    .ok_or_else(|| {
      Error::not_found(format!(
        "Douyin has no topic named #{name}; articles take existing topics only"
      ))
    })
}

/// How long a music is, in milliseconds (`music_end_time`).
async fn music_length(api: &Api, id: &str) -> Result<i64> {
  let v = api.get(MUSIC_DETAIL).arg("music_id", id).send().await?;
  v.f64("music_info.duration")
    .filter(|d| *d > 0.0)
    .map(|d| (d * 1000.0) as i64)
    .ok_or_else(|| {
      Error::not_found(format!(
        "no Douyin music {id}; `atp douyin musics` lists some"
      ))
    })
}

/// The form's `creation_id`: eight random letters and digits, then the
/// time in milliseconds.
fn creation_id() -> String {
  const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
  let mut rng = rand::rng();
  let head: String = (0..8)
    .map(|_| char::from(CHARS[rng.random_range(0..CHARS.len())]))
    .collect();
  format!("{head}{}", jiff::Timestamp::now().as_millisecond())
}

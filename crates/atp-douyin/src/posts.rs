//! One post: detail, downloadable media, related posts, likes (several
//! taken back at once).
//!
//! The detail comes from `/aweme/v1/web/aweme/detail/`. Visitors without a
//! browser `UIFID` are refused there; the share page's own API
//! (`iesdouyin.com/web/api/v2/aweme/iteminfo/`) answers them instead.

use atp_core::bulk;
use atp_core::http::Method;
use atp_core::{
  Action, Error, ErrorCode, Media, Page, PageReq, Post, Result, Value, ValueExt, json,
};

use crate::api::{Api, WWW};
use crate::{parse, refs, users};

const DETAIL: &str = "/aweme/v1/web/aweme/detail/";
const DIGG: &str = "/aweme/v1/web/commit/item/digg/";
const UNDIGG_MULTI: &str = "/aweme/v1/web/cancel/item/digg/multi/";
const RELATED: &str = "/aweme/v1/web/aweme/related/";
const ITEMINFO: &str = "/web/api/v2/aweme/iteminfo/";
/// Likes one `cancel/item/digg/multi` call takes back.
const BATCH: usize = 50;

/// The raw aweme of a post reference.
pub async fn detail(api: &Api, arg: &str) -> Result<Value> {
  let id = refs::aweme(api, arg).await?;
  let web = api
    .get(DETAIL)
    .arg("aweme_id", &id)
    .arg("version_code", "190500")
    .arg("version_name", "19.5.0")
    .send()
    .await;
  match web {
    Ok(v) if v.at("aweme_detail").is_object() => Ok(v.at("aweme_detail").clone()),
    Ok(v) => Err(missing(&v, &id)),
    // Visitors without a browser `UIFID` are refused here ("Uifid Not Found").
    Err(e)
      if matches!(
        e.code,
        ErrorCode::VerificationRequired | ErrorCode::PermissionDenied
      ) =>
    {
      share_page(api, &id).await.map_err(|_| e)
    }
    Err(e) => Err(e),
  }
}

/// Why a detail came back without an aweme.
fn missing(v: &Value, id: &str) -> Error {
  let reason = v
    .first_str(&[
      "filter_detail.notice",
      "filter_detail.detail_msg",
      "status_msg",
    ])
    .unwrap_or_else(|| "it was deleted or is private".to_owned());
  Error::not_found(format!("post {id} is not available: {reason}"))
}

/// The aweme as the share page loads it (`iesdouyin.com/web/api/v2/`).
async fn share_page(api: &Api, id: &str) -> Result<Value> {
  let v = api
    .get(ITEMINFO)
    .bare()
    .arg("reflow_source", "reflow_page")
    .arg("use_new_select_scope", "0")
    .arg("item_ids", id)
    .arg("aweme_type", "0")
    .arg("reflow_id", "")
    .send()
    .await?;
  v.list("item_list")
    .first()
    .cloned()
    .ok_or_else(|| Error::not_found(format!("post {id} is not available")))
}

pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let v = detail(api, arg).await?;
  parse::aweme(&v).ok_or_else(|| Error::upstream("Douyin returned a post without an id"))
}

/// The post and its media: the video (or every image), or the music alone.
pub async fn media(api: &Api, arg: &str, audio_only: bool) -> Result<(Post, Vec<Media>)> {
  let v = detail(api, arg).await?;
  let post =
    parse::aweme(&v).ok_or_else(|| Error::upstream("Douyin returned a post without an id"))?;
  let media = if audio_only {
    parse::music_media(&v).into_iter().collect()
  } else {
    let mut m = parse::media(&v);
    // An image post's soundtrack belongs to it.
    if parse::is_images(&v) {
      m.extend(parse::music_media(&v));
    }
    m
  };
  if media.is_empty() {
    return Err(Error::not_found("nothing to download in this post"));
  }
  Ok((post, media))
}

/// Posts Douyin recommends next to this one.
pub async fn related(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let id = refs::aweme(api, arg).await?;
  let v = api
    .get(RELATED)
    .arg("aweme_id", &id)
    .arg("count", page.size_within(20))
    .arg("filterGids", "")
    .arg("awemePcRecRawData", r#"{"is_client":false}"#)
    .arg("sub_channel_id", "3")
    .arg("refresh_index", page.number_or(1))
    .send()
    .await?;
  let items: Vec<Post> = v
    .list("aweme_list")
    .iter()
    .filter_map(parse::aweme)
    .collect();
  let next = (v.i64("has_more") == Some(1) && !items.is_empty())
    .then(|| (page.number_or(1) + 1).to_string());
  Ok(Page::new(items, next))
}

/// Like a post, or with `undo` take the like back.
pub async fn like(api: &Api, arg: &str, undo: bool) -> Result<Action> {
  let id = refs::aweme(api, arg).await?;
  let uid = users::uid_hash(api).await?;
  api
    .write(DIGG, false, &uid)
    .await?
    .header("referer", format!("{WWW}/discover?modal_id={id}"))
    .field("aweme_id", &id)
    .field("item_type", "0")
    .field("type", if undo { "0" } else { "1" })
    .send()
    .await?;
  let name = if undo { "unlike" } else { "like" };
  Ok(Action::done(name, &id).with_url(refs::video_url(&id)))
}

/// Like several posts one by one, or take several likes back 50 per call
/// as the likes tab's 取消喜欢 does; the ones it names in `fatal_ids`
/// failed.
pub async fn like_posts(api: &Api, posts: &[String], undo: bool) -> Result<Vec<Action>> {
  if !undo {
    return bulk::each("like", posts, async |post| like(api, post, false).await).await;
  }
  let ids = refs::awemes(api, posts).await?;
  let me = users::uid_hash(api).await?;
  bulk::batches("unlike", &ids, BATCH, String::clone, async |chunk| {
    // Ordinary posts are type 0 (西瓜 videos would be 1).
    let types: serde_json::Map<String, Value> =
      chunk.iter().map(|id| (id.clone(), json!(0))).collect();
    let v = api
      .write_as(Method::POST, UNDIGG_MULTI, &me)
      .await?
      .header("referer", format!("{WWW}/user/self?showTab=like"))
      .field("aweme_ids", chunk.join(","))
      .field("item_type_map", Value::Object(types))
      .send()
      .await?;
    Ok(parse::cancelled(&v, "unlike", chunk, |id| {
      Some(refs::video_url(id))
    }))
  })
  .await
}

//! Reading posts through their mobile pages, and the video files behind
//! them: a post's `playAuthTokenV2` is a presigned VOD query whose answer
//! lists progressive MP4s with audio.

use atp_core::{Error, Media, MediaKind, Post, Result, Value, ValueExt};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use crate::api::Api;
use crate::parse::article;
use crate::refs;

const VOD: &str = "https://vod.bytedanceapi.com/";

pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let wanted = refs::post(api, arg).await?;
  let resp = api.mobile_page(&refs::mobile_url(&wanted)).await?;
  let data = article::render_data(&resp.text())?;
  let mut post = article::post(&data, &wanted)?;
  if let Some(token) = post
    .extra
    .get("play_token")
    .and_then(Value::as_str)
    .map(str::to_owned)
  {
    match video(api, &token).await {
      Ok(media) => post.media.insert(0, media),
      Err(e) => tracing::debug!("video of {}: {e}", post.id),
    }
  }
  post.extra.remove("play_token");
  Ok(post)
}

/// The best stream of a video, from its play token.
async fn video(api: &Api, token: &str) -> Result<Media> {
  let raw = STANDARD
    .decode(token.trim())
    .map_err(|_| Error::upstream("unreadable video token"))?;
  let token: Value = serde_json::from_slice(&raw)?;
  let query = token
    .str("GetPlayInfoToken")
    .ok_or_else(|| Error::upstream("the video token has no play query"))?
    .replace("\\u0026", "&");
  let v = api
    .ctx
    .http
    .get(format!("{VOD}?{query}"))
    .no_cookies()
    .value()
    .await?;
  let data = v.at("Result.Data");
  let best = data
    .list("PlayInfoList")
    .iter()
    .max_by_key(|p| p.u64("Bitrate").unwrap_or(0))
    .ok_or_else(|| Error::upstream("the video has no playable stream"))?;
  let url = best
    .first_str(&["MainPlayUrl", "BackupPlayUrl"])
    .ok_or_else(|| Error::upstream("the video stream has no address"))?;
  let mut media = Media::new(MediaKind::Video, url);
  media.width = best.u64("Width").map(|w| w as u32);
  media.height = best.u64("Height").map(|h| h as u32);
  media.size = best.u64("Size");
  media.duration = data.f64("Duration");
  Ok(media)
}

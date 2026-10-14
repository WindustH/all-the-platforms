//! One post: the detail query of the short-video page, or — when Kuaishou
//! puts that behind a slider — the mobile share page, which answers
//! visitors. Also the posts Kuaishou recommends next to one.

use atp_core::{Error, ErrorCode, Media, Page, PageReq, Post, Result, ValueExt, json, paging};

use crate::api::{Api, H5};
use crate::{parse, refs};

/// Verbatim from the web (the server checks the text).
const VIDEO_DETAIL: &str = "query visionVideoDetail($photoId: String, $type: String, $page: String, $webPageArea: String) {\n  visionVideoDetail(photoId: $photoId, type: $type, page: $page, webPageArea: $webPageArea) {\n    status\n    type\n    author {\n      id\n      name\n      following\n      headerUrl\n      livingInfo\n      verifiedDetail\n      __typename\n    }\n    photo {\n      id\n      duration\n      caption\n      likeCount\n      realLikeCount\n      coverUrl\n      photoUrl\n      liked\n      timestamp\n      expTag\n      llsid\n      viewCount\n      videoRatio\n      stereoType\n      musicBlocked\n      riskTagContent\n      riskTagUrl\n      manifest {\n        mediaType\n        businessType\n        version\n        adaptationSet {\n          id\n          duration\n          representation {\n            id\n            defaultSelect\n            backupUrl\n            codecs\n            url\n            height\n            width\n            avgBitrate\n            maxBitrate\n            m3u8Slice\n            qualityType\n            qualityLabel\n            frameRate\n            featureP2sp\n            hidden\n            disableAdaptive\n            __typename\n          }\n          __typename\n        }\n        __typename\n      }\n      manifestH265\n      photoH265Url\n      coronaCropManifest\n      coronaCropManifestH265\n      croppedPhotoH265Url\n      croppedPhotoUrl\n      videoResource\n      __typename\n    }\n    authorStatement {\n      content\n      type\n      riskStyleType\n      __typename\n    }\n    tags {\n      type\n      name\n      __typename\n    }\n    commentLimit {\n      canAddComment\n      __typename\n    }\n    llsid\n    danmakuSwitch\n    __typename\n  }\n}\n";

const SHORT_VIDEO_RECO: &str = "fragment photoContent on PhotoEntity {\n  __typename\n  id\n  duration\n  caption\n  originCaption\n  likeCount\n  viewCount\n  commentCount\n  realLikeCount\n  coverUrl\n  photoUrl\n  photoH265Url\n  manifest\n  manifestH265\n  videoResource\n  coverUrls {\n    url\n    __typename\n  }\n  timestamp\n  expTag\n  animatedCoverUrl\n  distance\n  disableSensitivePhoto\n  videoRatio\n  liked\n  stereoType\n  profileUserTopPhoto\n  musicBlocked\n  riskTagContent\n  riskTagUrl\n}\n\nquery visionShortVideoReco($semKeyword: String, $semCrowd: String, $utmSource: String, $utmMedium: String, $page: String, $photoId: String, $utmCampaign: String) {\n  visionShortVideoReco(semKeyword: $semKeyword, semCrowd: $semCrowd, utmSource: $utmSource, utmMedium: $utmMedium, page: $page, photoId: $photoId, utmCampaign: $utmCampaign) {\n    llsid\n    feeds {\n      type\n      author {\n        id\n        name\n        following\n        headerUrl\n        __typename\n      }\n      photo {\n        ...photoContent\n        __typename\n      }\n      tags {\n        type\n        name\n        __typename\n      }\n      authorStatement {\n        content\n        type\n        riskStyleType\n        __typename\n      }\n      canAddComment\n      __typename\n    }\n    __typename\n  }\n}\n";

pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  let id = refs::photo(api, arg).await?;
  match detail(api, &id).await {
    Err(e) if e.code == ErrorCode::VerificationRequired => {
      tracing::debug!("detail behind a captcha, reading the share page: {e}");
      share_page(api, &id).await.map_err(|share| {
        tracing::debug!("share page failed too: {share}");
        e
      })
    }
    other => other,
  }
}

async fn detail(api: &Api, id: &str) -> Result<Post> {
  let data = api
    .graphql(
      "visionVideoDetail",
      json!({ "photoId": id, "page": "detail" }),
      VIDEO_DETAIL,
      &refs::photo_url(id),
    )
    .await?;
  let detail = data.at("visionVideoDetail");
  match detail.i64("status") {
    Some(1) => {
      parse::post(detail).ok_or_else(|| Error::upstream("Kuaishou: no photo in the answer"))
    }
    status => Err(Error::not_found(format!(
      "Kuaishou post {id} is gone or hidden (status {})",
      status.unwrap_or(-1)
    ))),
  }
}

/// The mobile share page (`/fw/photo/<id>`): its `INIT_STATE` holds the photo.
async fn share_page(api: &Api, id: &str) -> Result<Post> {
  let html = api.page(&format!("{H5}/fw/photo/{id}"), true).await?;
  let state = parse::page_state(&html, "window.INIT_STATE")
    .ok_or_else(|| Error::upstream("Kuaishou share page: no INIT_STATE"))?;
  let photo = parse::share_photo(&state)
    .ok_or_else(|| Error::not_found(format!("Kuaishou post {id} is gone or hidden")))?;
  let mut post =
    parse::post(photo).ok_or_else(|| Error::upstream("Kuaishou share page: no photo"))?;
  // The page's photoId is numeric; keep the id links use.
  post.extra.insert("photo_id".into(), post.id.clone().into());
  post.id = id.to_owned();
  post.url = Some(refs::photo_url(id));
  Ok(post)
}

pub async fn media(api: &Api, arg: &str, audio_only: bool) -> Result<(Post, Vec<Media>)> {
  if audio_only {
    return Err(Error::unsupported("download --audio-only"));
  }
  let post = read(api, arg).await?;
  let media = post.media.clone();
  Ok((post, media))
}

/// The posts the detail page recommends next to one (one batch).
pub async fn related(api: &Api, arg: &str, page: &PageReq) -> Result<Page<Post>> {
  let id = refs::photo(api, arg).await?;
  let data = api
    .graphql(
      "visionShortVideoReco",
      json!({ "page": "detail", "photoId": id }),
      SHORT_VIDEO_RECO,
      &refs::photo_url(&id),
    )
    .await?;
  let posts = data
    .list("visionShortVideoReco.feeds")
    .iter()
    .filter_map(parse::post)
    .collect();
  Ok(paging::slice(posts, page))
}

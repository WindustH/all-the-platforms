//! `read` and `download`. Goods ids read the goods page with a login (all
//! SKUs, gallery, videos, shop), else go through 多多进宝 (`goods_sign`,
//! `pdd.ddk.goods.detail`). `download` saves a goods' gallery and videos,
//! the pictures and video of one review (`GOODS/REVIEW`, or a review link
//! with `goods_id` and `review_id`), or the image a share link carries
//! (`_oak_share_url`), which needs no login.

use atp_core::{Error, Media, Post, Result, Value, ValueExt};

use crate::api::{Api, goods_url, image_url};
use crate::comments;
use crate::refs::{self, Goods};
use crate::{parse, search};

/// Review pages scanned for one review.
const REVIEW_PAGES: u64 = 5;

pub async fn read(api: &Api, post: &str) -> Result<Post> {
  match refs::goods(api, post).await? {
    Goods::Sign(sign) => official(api, &sign).await,
    Goods::Id(id) if api.has_session() => page(api, &id).await,
    Goods::Id(id) if api.ddk.has(&api.ctx) => {
      let sign = search::sign_of(api, &id).await?;
      official(api, &sign).await
    }
    Goods::Id(_) => Err(
      Error::auth("reading goods needs a login or 多多进宝 credentials")
        .with_hint(api.either_hint()),
    ),
  }
}

/// The goods page of a logged-in session.
async fn page(api: &Api, id: &str) -> Result<Post> {
  let store = api.store(&goods_url(id)).await?;
  parse::detail(&store, id).ok_or_else(|| Error::not_found(format!("Pinduoduo has no goods {id}")))
}

/// `pdd.ddk.goods.detail` of a `goods_sign`.
async fn official(api: &Api, sign: &str) -> Result<Post> {
  let pid = api.ddk.creds(&api.ctx).map(|c| c.pid).unwrap_or_default();
  let v = api
    .ddk
    .call(
      &api.ctx,
      "pdd.ddk.goods.detail",
      &[("goods_sign", sign.to_owned()), ("pid", pid)],
    )
    .await?;
  parse::ddk_goods(v.at("goods_detail_response.goods_details.0"))
    .ok_or_else(|| Error::not_found(format!("多多进宝 has no goods {sign}")))
}

/// The post and files of `download`.
pub async fn media(api: &Api, post: &str) -> Result<(Post, Vec<Media>)> {
  if let Some(image) = shared_image(api, post).await {
    let post = Post {
      id: post.to_owned(),
      kind: "image".into(),
      url: Some(post.to_owned()),
      media: vec![Media::image(image)],
      ..Post::default()
    };
    let media = post.media.clone();
    return Ok((post, media));
  }
  if let Some((goods, review)) = review_ref(post) {
    return review_media(api, &goods, &review).await;
  }
  let post = read(api, post).await?;
  let media = post.media.clone();
  Ok((post, media))
}

/// The image of a share link (`_oak_share_url`), following short links.
async fn shared_image(api: &Api, arg: &str) -> Option<String> {
  if !arg.starts_with("http") {
    return None;
  }
  let found = |link: &str| {
    url::Url::parse(link)
      .ok()?
      .query_pairs()
      .find(|(k, _)| k == "_oak_share_url")
      .map(|(_, v)| image_url(&v))
  };
  if let Some(image) = found(arg) {
    return Some(image);
  }
  if !arg.contains("ps=") {
    return None;
  }
  found(&api.ctx.http.final_url(arg).await.ok()?)
}

/// `GOODS/REVIEW`, or a link with `goods_id` and `review_id`.
fn review_ref(arg: &str) -> Option<(String, String)> {
  if let Some((goods, review)) = arg.split_once('/')
    && !arg.contains(':')
    && [goods, review]
      .iter()
      .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
  {
    return Some((goods.to_owned(), review.to_owned()));
  }
  let url = url::Url::parse(arg).ok()?;
  let param = |name: &str| {
    url
      .query_pairs()
      .find(|(k, _)| k == name)
      .map(|(_, v)| v.into_owned())
  };
  Some((param("goods_id")?, param("review_id")?))
}

/// One review's pictures and video, found in the first review pages.
async fn review_media(api: &Api, goods: &str, review: &str) -> Result<(Post, Vec<Media>)> {
  api.require_session()?;
  for number in 1..=REVIEW_PAGES {
    let v = comments::list_page(api, goods, number).await?;
    let list = v.list("data");
    if let Some(found) = list
      .iter()
      .find(|r| r.str("review_id").as_deref() == Some(review))
    {
      return Ok(review_post(found, goods, review));
    }
    if list.is_empty() {
      break;
    }
  }
  Err(Error::not_found(format!(
    "review {review} is not among the first reviews of goods {goods}"
  )))
}

fn review_post(v: &Value, goods: &str, review: &str) -> (Post, Vec<Media>) {
  let comment = parse::review(v);
  let mut media: Vec<Media> = comment
    .as_ref()
    .and_then(|c| c.extra.get("images"))
    .and_then(Value::as_array)
    .map(|list| {
      list
        .iter()
        .filter_map(|u| u.as_str())
        .map(Media::image)
        .collect()
    })
    .unwrap_or_default();
  if let Some(video) = comment
    .as_ref()
    .and_then(|c| c.extra.get("video"))
    .and_then(Value::as_str)
  {
    media.push(Media::video(video));
  }
  let post = Post {
    id: format!("{goods}/{review}"),
    kind: "review".into(),
    text: comment.map(|c| c.text),
    url: Some(goods_url(goods)),
    media: media.clone(),
    raw: Some(v.clone()),
    ..Post::default()
  };
  (post, media)
}

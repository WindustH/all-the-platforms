//! `read` and `download` of any reference (products go to [`products`]),
//! community posts from their share pages (`m.dewu.com`, no signature),
//! and `feed`: the PC community page's hot list (signed GET).

use atp_core::{Error, Media, Page, PageReq, Post, Result, ValueExt, json};

use crate::api::{Api, COMMUNITY};
use crate::refs::{self, Ref};
use crate::{parse, products};

const HOT: &str = "/sns-rec/v1/common/h5/community-home-hot-content";
/// Posts per page of the hot list, as the community page asks.
const FEED_SIZE: u64 = 32;

pub async fn read(api: &Api, arg: &str) -> Result<Post> {
  match refs::parse(api, arg).await? {
    Ref::Product(spu) => products::read(api, &spu).await,
    Ref::Trend(id) => trend(api, &id).await,
  }
}

/// The files of `download`: a post's video or images, a product's
/// gallery (without asking for prices).
pub async fn media(api: &Api, arg: &str) -> Result<(Post, Vec<Media>)> {
  let post = match refs::parse(api, arg).await? {
    Ref::Product(spu) => products::detail(api, &spu).await?,
    Ref::Trend(id) => trend(api, &id).await?,
  };
  let media = post.media.clone();
  Ok((post, media))
}

/// A community post from its share page.
async fn trend(api: &Api, id: &str) -> Result<Post> {
  let next = api.share_page(id).await?;
  let info = next.at("props.pageProps.metaOGInfo");
  let code = info.i64("code").or_else(|| info.i64("status"));
  if code.is_some_and(|c| c != 200) {
    let text = info.str("msg").unwrap_or_default();
    return Err(Error::upstream(format!(
      "Dewu post {id}: {text} (code {})",
      code.unwrap_or_default()
    )));
  }
  parse::shared(info, id)
    .ok_or_else(|| Error::not_found(format!("Dewu has no post {id} (deleted or private)")))
}

/// The hot list; cursors are page numbers.
pub async fn feed(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  let number = page.number_or(1);
  let params = json!({ "page": number, "pageSize": FEED_SIZE });
  let v = api.get(HOT, params, COMMUNITY).await?;
  let items: Vec<Post> = v
    .list("data.contentList")
    .iter()
    .filter_map(parse::hot_content)
    .collect();
  let pages = v.u64("data.total").map(|t| t.div_ceil(FEED_SIZE));
  let more = !items.is_empty() && pages.is_none_or(|p| number < p);
  Ok(Page::new(items, more.then(|| (number + 1).to_string())))
}

//! Shop reviews from Dianping's review list
//! (`mapi.dianping.com/mapi/review/outsidesiftedreviewlist.bin`). Meituan's
//! 到店 shops show the same reviews (its app reads them from Dianping's
//! backend), so both platforms list them here.
//!
//! The list wants a logged-in Dianping session (`dper`) and the shop's www
//! page as `Referer`. A page is 14 reviews; the cursor is the offset
//! (`start`). Merchant replies (`商家回应`) are not reviews and are left
//! out.

use atp_core::text::{from_beijing, from_unix, html_to_text};
use atp_core::{Comment, Ctx, Error, Page, PageReq, Result, User, Value, ValueExt};
use rand::Rng;

use crate::guard::{self, Call, DIANPING};
use crate::shops;

/// Reviews a page of the list holds.
pub const STEP: u64 = 14;
const LIST: &str = "https://mapi.dianping.com/mapi/review/outsidesiftedreviewlist.bin";
/// The 全部 tab.
const ALL: &str = "800";
const MERCHANT: &str = "商家回应";
const LOGIN_COOKIE: &str = "dper";

/// One page of the reviews of the shop with this uuid.
pub async fn list(ctx: &Ctx, uuid: &str, page: &PageReq) -> Result<Page<Comment>> {
  if !ctx.http.has_cookie(LOGIN_COOKIE) {
    return Err(
      Error::auth("Dianping shows reviews after a login (no `dper` cookie)")
        .with_hint(ctx.login_hint()),
    );
  }
  let start = page.number_or(0);
  let call = Call::xhr(LIST, shops::www_url(uuid))
    .query("optimus_code", "10")
    .query("optimus_partner", "76")
    .query("optimus_risk_level", "71")
    .query("reqsource", "4")
    .query("filterid", ALL)
    .query("merge", "1")
    .query("needfilter", "1")
    .query("queryid", query_id())
    .query("referid", uuid)
    .query("refertype", "0")
    .query("start", start)
    .query("multifilterids", format!(r#"{{"filterIds":[{ALL}]}}"#))
    .h5();
  let v = guard::json(ctx, &DIANPING, call).await?;
  let entries = v.list("list");
  let items: Vec<Comment> = entries.iter().filter_map(review).collect();
  let end = entries.is_empty() || v.bool("isEnd").unwrap_or(false);
  let next = (!end).then(|| (start + STEP).to_string());
  Ok(Page::new(items, next))
}

/// `<epoch ms>_<random [a-z0-9]>`, as the page names its queries.
fn query_id() -> String {
  const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
  let mut rng = rand::rng();
  let tail: String = (0..12)
    .map(|_| CHARS[rng.random_range(0..CHARS.len())] as char)
    .collect();
  format!("{}_{tail}", jiff::Timestamp::now().as_millisecond())
}

/// A review of the list; `None` for merchant replies and other entries.
pub fn review(v: &Value) -> Option<Comment> {
  let name = v.first_str(&["feedUser.userName", "user.userName", "userName"])?;
  if name == MERCHANT {
    return None;
  }
  let text = v.str("content").unwrap_or_default();
  let text = if text.contains('<') {
    html_to_text(&text)
  } else {
    text
  };
  let created_at = time(v);
  let id = v
    .first_str(&["reviewId", "feedId", "mainId", "id", "contentId"])
    .unwrap_or_else(|| {
      let at = created_at.map(|t| t.as_second()).unwrap_or_default();
      format!("{name}-{at}")
    });
  let mut comment = Comment {
    id,
    author: Some(author(v, name)),
    text,
    created_at,
    likes: v.first_count(&["likeCount", "flowerCount", "praiseCount", "likeNum"]),
    reply_count: v.first_count(&["commentCount", "replyCount"]),
    raw: Some(v.clone()),
    ..Comment::default()
  };
  extra(&mut comment, v);
  Some(comment)
}

fn author(v: &Value, name: String) -> User {
  let id = v
    .first_str(&["feedUser.userId", "feedUser.id", "user.userId", "userId"])
    .unwrap_or_default();
  User {
    url: (!id.is_empty()).then(|| format!("{}/member/{id}", shops::WWW)),
    avatar: v.first_str(&["feedUser.userFace", "feedUser.avatar", "feedUser.face"]),
    id,
    name,
    ..User::default()
  }
}

fn time(v: &Value) -> Option<jiff::Timestamp> {
  for key in ["addTime", "time", "publishTime", "createTime", "lastTime"] {
    if let Some(n) = v.i64(key) {
      return from_unix(n);
    }
    if let Some(t) = v.str(key).and_then(|s| from_beijing(&s)) {
      return Some(t);
    }
  }
  None
}

/// Stars, sub-scores, spend per person and pictures.
fn extra(comment: &mut Comment, v: &Value) {
  let extra = &mut comment.extra;
  if let Some(star) = v.first_str(&["star", "accurateStar", "reviewStar"]) {
    extra.insert("star".into(), star.into());
  }
  let scores: Vec<Value> = v
    .list("feedScoreList")
    .iter()
    .filter_map(|s| s.str("text").map(Value::from))
    .collect();
  if !scores.is_empty() {
    extra.insert("scores".into(), scores.into());
  }
  if let Some(price) = v.first_str(&["avgPrice", "price", "priceText"]) {
    extra.insert("avg_price".into(), price.into());
  }
  let images: Vec<Value> = ["picList", "reviewPics", "pics"]
    .iter()
    .flat_map(|k| v.list(k))
    .filter_map(|p| p.first_str(&["bigPicture", "bigurl", "url", "picUrl", "thumbUrl"]))
    .map(Value::from)
    .collect();
  if !images.is_empty() {
    extra.insert("images".into(), images.into());
  }
}

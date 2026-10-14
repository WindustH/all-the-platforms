//! Creator-center numbers: the account overview, a post's counters from
//! the works list, and that list itself (with review states).

use atp_core::{Error, Insights, Page, PageReq, Post, Result, Value, ValueExt, json};

use crate::api::{Api, CP};
use crate::{login, parse, refs, users};

const OVERVIEW: &str = "/rest/cp/creator/analysis/pc/home/author/overview";
const WORKS: &str = "/rest/cp/works/v2/video/pc/photo/list";
/// The works list answers one year at a time, 20 rows a page.
const YEAR_MS: u64 = 365 * 86_400_000;
const WORKS_PAGE: u64 = 20;
/// How many pages of works a post's insights look through.
const WORKS_SEARCH_PAGES: usize = 5;

/// The overview of the creator center's home (its default window).
pub async fn account(api: &Api) -> Result<Insights> {
  login::cp_tickets(api).await?;
  let me = users::whoami(api).await?;
  let v = api
    .cp(OVERVIEW)
    .json(json!({ "timeType": 3 }))
    .send()
    .await?;
  let mut ins = Insights {
    kind: "account".into(),
    subject: me.id.clone(),
    title: Some(me.name.clone()),
    url: me.url.clone(),
    totals_period: Some("the creator center's overview window".into()),
    raw: Some(v.clone()),
    ..Insights::default()
  };
  totals(&mut ins, "", v.at("data"), 0);
  Ok(ins)
}

/// Numeric fields of the overview as totals, nested ones as `a.b`.
fn totals(ins: &mut Insights, prefix: &str, v: &Value, depth: usize) {
  let Some(obj) = v.as_object() else {
    return;
  };
  for (key, value) in obj {
    let name = if prefix.is_empty() {
      key.clone()
    } else {
      format!("{prefix}.{key}")
    };
    match value {
      Value::Number(_) => ins.total(&name, value.clone()),
      Value::Object(_) if depth < 2 => totals(ins, &name, value, depth + 1),
      _ => {}
    }
  }
}

/// A post's counters, from its row in the works list.
pub async fn post(api: &Api, arg: &str) -> Result<Insights> {
  let id = refs::photo(api, arg).await?;
  let mut req = PageReq {
    cursor: None,
    size: WORKS_PAGE as usize,
  };
  for _ in 0..WORKS_SEARCH_PAGES {
    let page = works(api, &req).await?;
    if let Some(post) = page.items.into_iter().find(|w| w.id == id) {
      return Ok(post_insights(post));
    }
    match page.next_cursor {
      Some(next) => req.cursor = Some(next),
      None => break,
    }
  }
  Err(Error::not_found(format!(
    "post {id} is not among your recent works (insights cover your own posts)"
  )))
}

fn post_insights(post: Post) -> Insights {
  let mut ins = Insights {
    kind: "post".into(),
    title: post.text.clone(),
    url: post.url.clone(),
    subject: post.id.clone(),
    ..Insights::default()
  };
  let m = &post.metrics;
  for (name, value) in [
    ("views", m.views),
    ("likes", m.likes),
    ("comments", m.comments),
    ("shares", m.shares),
    ("favorites", m.favorites),
  ] {
    ins.total(name, value.map(Value::from));
  }
  ins.extra = post.extra.clone();
  ins.raw = post.raw;
  ins
}

/// The account's works, newest first, within the past year. The cursor is
/// the works list's own (a time in ms).
pub async fn works(api: &Api, page: &PageReq) -> Result<Page<Post>> {
  login::cp_tickets(api).await?;
  let now = crate::sign::now_ms();
  let cursor: u64 = page
    .cursor
    .as_deref()
    .and_then(|c| c.parse().ok())
    .unwrap_or(now);
  // The page's own field order.
  let body = json!({
    "queryType": "0",
    "cursor": cursor,
    "startTime": now - YEAR_MS,
    "endTime": now,
    "limit": WORKS_PAGE,
    "timeRangeType": 5,
    "keyword": "",
  });
  let v = api
    .cp(WORKS)
    .json(body)
    .referer(&format!("{CP}/article/manage/video"))
    .send()
    .await?;
  let rows = v.list("data.list");
  let posts: Vec<Post> = rows.iter().filter_map(parse::work).collect();
  let next = parse::more(v.str("data.nextCursor")).filter(|_| rows.len() as u64 >= WORKS_PAGE);
  Ok(Page::new(posts, next))
}

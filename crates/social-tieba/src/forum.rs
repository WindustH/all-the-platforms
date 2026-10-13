//! Forums (吧) and discovery: a forum's threads and details, hot topics and
//! their threads, and search over threads, forums and users.

use social_core::{Collection, Error, Page, PageReq, Post, Query, Result, User, Value, ValueExt};

use crate::api::{Client, Params};
use crate::{parse, refs};

/// Orders of a forum's thread list.
pub const SORTS: &[&str] = &["reply", "new", "good"];
pub const SEARCH_SORTS: &[&str] = &["relevant", "new", "old"];
/// `threads`: only thread starts; `posts`: replies too.
pub const SEARCH_FILTERS: &[&str] = &["threads", "posts"];

/// A forum page (`frs`): the forum and one page of its threads.
async fn frs(c: &Client, name: &str, sort: &str, pn: u64, rn: usize) -> Result<Value> {
  let mut params: Params = vec![
    ("kw", name.to_owned()),
    ("pn", pn.to_string()),
    ("rn", rn.to_string()),
    ("sort_type", if sort == "new" { "1" } else { "0" }.into()),
    ("q_type", "2".into()),
    ("st_type", "tb_forumlist".into()),
    ("with_group", "0".into()),
    ("scr_dip", "3.0".into()),
    ("scr_h", "2400".into()),
    ("scr_w", "1080".into()),
  ];
  if sort == "good" {
    params.push(("is_good", "1".into()));
    params.push(("cid", "0".into()));
  }
  let v = c.call("/c/f/frs/page", params).await?;
  // `forum.is_exists` says 0 for the newest-first order; the id tells.
  if v.str("forum.id").filter(|id| id != "0").is_none() {
    return Err(Error::not_found(format!("no Tieba forum named {name}")));
  }
  Ok(v)
}

/// One page of a forum's threads, by last reply, creation or 精品 only.
pub async fn threads(c: &Client, arg: &str, sort: &str, req: &PageReq) -> Result<Page<Post>> {
  let name = refs::forum(arg)?;
  let pn = req.number_or(1);
  let v = frs(c, &name, sort, pn, req.size_within(30)).await?;
  let users = parse::users(v.list("user_list"));
  let forum = v.str("forum.name").unwrap_or(name);
  let items: Vec<Post> = v
    .list("thread_list")
    .iter()
    .map(|t| {
      let mut p = parse::thread(t, &users);
      p.extra
        .entry("forum".into())
        .or_insert_with(|| forum.clone().into());
      p
    })
    .collect();
  let more = v.str("page.has_more").as_deref() == Some("1") && !items.is_empty();
  Ok(Page::new(items, more.then(|| (pn + 1).to_string())))
}

/// A forum's details: members, threads, posts, category, slogan.
pub async fn info(c: &Client, arg: &str) -> Result<Collection> {
  let name = refs::forum(arg)?;
  let v = frs(c, &name, "reply", 1, 1).await?;
  let mut forum = parse::forum(v.at("forum"))
    .ok_or_else(|| Error::upstream(format!("no details for forum {name}")))?;
  let managers: Vec<String> = v
    .list("forum.managers")
    .iter()
    .filter_map(|m| m.first_str(&["show_name", "name"]))
    .collect();
  if !managers.is_empty() {
    forum.extra.insert("managers".into(), managers.into());
  }
  Ok(forum)
}

/// Id and canonical name of a forum, which writes need.
pub async fn fid(c: &Client, arg: &str) -> Result<(String, String)> {
  let name = refs::forum(arg)?;
  let v = frs(c, &name, "reply", 1, 1).await?;
  let fid = v
    .str("forum.id")
    .ok_or_else(|| Error::upstream(format!("no id for forum {name}")))?;
  Ok((fid, v.str("forum.name").unwrap_or(name)))
}

/// The hot topic board (热议榜).
pub async fn hot(c: &Client) -> Result<Page<Post>> {
  let v = c.web("/hottopic/browse/topicList", &[]).await?;
  let items = v
    .list("data.bang_topic.topic_list")
    .iter()
    .filter_map(parse::topic)
    .collect();
  Ok(Page::last(items))
}

/// One page of the threads of a hot topic.
pub async fn topic(c: &Client, arg: &str, req: &PageReq) -> Result<Page<Post>> {
  let (id, name) = refs::topic(arg)?;
  let pn = req.number_or(1);
  let v = c
    .web(
      "/mo/q/newtopic/topicDetail",
      &[
        ("topic_id", id),
        ("topic_name", name),
        ("is_new", "1".into()),
        ("is_share", "1".into()),
        ("pn", pn.to_string()),
        ("rn", req.size_within(30).to_string()),
        ("offset", "0".into()),
        ("last_id", String::new()),
      ],
    )
    .await?;
  let none = parse::Users::new();
  let items: Vec<Post> = v
    .list("data.relate_thread.thread_list")
    .iter()
    .map(|t| parse::thread(t.at("thread_info"), &none))
    .collect();
  let more = v.str("data.has_more").as_deref() == Some("1") && !items.is_empty();
  Ok(Page::new(items, more.then(|| (pn + 1).to_string())))
}

/// Thread search, over all forums or inside one.
pub async fn search(
  c: &Client,
  q: &Query,
  forum: Option<&str>,
  req: &PageReq,
) -> Result<Page<Post>> {
  let pn = req.number_or(1);
  let st = match q.sort.as_deref() {
    Some("new") => "1",
    Some("old") => "0",
    _ => "2",
  };
  let tt = if q.filter.as_deref() == Some("posts") {
    "2"
  } else {
    "1"
  };
  let mut query = vec![
    ("word", q.keyword.clone()),
    ("pn", pn.to_string()),
    ("st", st.into()),
    ("tt", tt.into()),
    ("ct", "1".into()),
    ("rn", req.size_within(30).to_string()),
    ("is_use_zonghe", "1".into()),
    ("cv", "99.9.101".into()),
  ];
  if let Some(f) = forum {
    query.push(("fname", refs::forum(f)?));
  }
  let v = c.web("/mo/q/search/thread", &query).await?;
  let none = parse::Users::new();
  let items: Vec<Post> = v
    .list("data.post_list")
    .iter()
    .map(|t| parse::thread(t, &none))
    .collect();
  let more = v.str("data.has_more").as_deref() == Some("1") && !items.is_empty();
  Ok(Page::new(items, more.then(|| (pn + 1).to_string())))
}

/// Exact match first, then the fuzzy ones.
fn matches(v: &Value) -> Vec<&Value> {
  let exact = v.at("data.exactMatch");
  std::iter::once(exact)
    .filter(|e| e.is_object())
    .chain(v.list("data.fuzzyMatch"))
    .collect()
}

pub async fn search_forums(c: &Client, q: &Query) -> Result<Page<Collection>> {
  let v = c
    .web("/mo/q/search/forum", &[("word", q.keyword.clone())])
    .await?;
  Ok(Page::last(
    matches(&v).into_iter().filter_map(parse::forum).collect(),
  ))
}

pub async fn search_users(c: &Client, q: &Query) -> Result<Page<User>> {
  let v = c
    .web("/mo/q/search/user", &[("word", q.keyword.clone())])
    .await?;
  Ok(Page::last(
    matches(&v).into_iter().filter_map(parse::user).collect(),
  ))
}

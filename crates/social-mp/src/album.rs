//! Albums (合集): the one anonymous listing of an account's articles.
//!
//! `mp/appmsgalbum?action=getalbum&f=json` pages forward from the last item
//! (`begin_msgid` + `begin_itemidx`); `continue_flag` "0" ends it. A page
//! with a single item sends `article_list` as an object instead of an array.

use social_core::text::from_secs;
use social_core::{Collection, Ctx, Error, Page, PageReq, Post, Result, User, Value, ValueExt};

use crate::refs::{self, HOME};

pub async fn page(ctx: &Ctx, arg: &str, req: &PageReq) -> Result<(Collection, Page<Post>)> {
  let (biz, album_id) = refs::album(arg)?;
  let count = req.size_within(20).to_string();
  let mut query = vec![
    ("action", "getalbum".to_owned()),
    ("__biz", biz.clone()),
    ("album_id", album_id.clone()),
    ("count", count),
    ("f", "json".to_owned()),
  ];
  if let Some((msgid, itemidx)) = req.cursor.as_deref().and_then(|c| c.split_once('_')) {
    query.push(("begin_msgid", msgid.to_owned()));
    query.push(("begin_itemidx", itemidx.to_owned()));
  }
  let v = ctx
    .http
    .get(format!("{HOME}/mp/appmsgalbum"))
    .queries(query)
    .value()
    .await?;
  match v.i64("base_resp.ret") {
    Some(0) | None => {}
    Some(ret) => return Err(Error::upstream(format!("album listing failed (ret {ret})"))),
  }
  let resp = v.at("getalbum_resp");
  let items: Vec<Value> = match resp.at("article_list") {
    Value::Array(a) => a.clone(),
    Value::Object(_) => vec![resp.at("article_list").clone()],
    _ => Vec::new(),
  };
  let info = resp.at("base_info");
  let owner = User {
    id: info.str("username").unwrap_or_default(),
    name: info.str("nickname").unwrap_or_default(),
    ..User::default()
  };
  let posts: Vec<Post> = items.iter().map(|a| item(a, &owner)).collect();
  let next = (resp.str("continue_flag").as_deref() == Some("1"))
    .then(|| {
      items.last().map(|l| {
        format!(
          "{}_{}",
          l.str("msgid").unwrap_or_default(),
          l.str("itemidx").unwrap_or_default()
        )
      })
    })
    .flatten();
  let album = Collection {
    id: album_id.clone(),
    kind: "album".into(),
    name: info.str("title").unwrap_or_default(),
    description: info.str("description"),
    url: Some(refs::album_url(&biz, &album_id)),
    items: info.u64("article_count"),
    views: info.u64("read_count"),
    owner: Some(owner).filter(|o| !o.name.is_empty()),
    ..Collection::default()
  };
  Ok((album, Page::new(posts, next)))
}

fn item(a: &Value, owner: &User) -> Post {
  let url = a.str("url").and_then(|u| refs::article_url(&u).ok());
  let mut post = Post {
    id: a.str("key").unwrap_or_default(),
    kind: "article".into(),
    title: a.str("title"),
    url,
    author: Some(owner.clone()).filter(|o| !o.name.is_empty()),
    created_at: a.i64("create_time").and_then(from_secs),
    raw: Some(a.clone()),
    ..Post::default()
  };
  for (key, path) in [("cover", "cover_img_1_1"), ("position", "pos_num")] {
    if let Some(v) = a.str(path) {
      post.extra.insert(key.into(), v.into());
    }
  }
  post
}

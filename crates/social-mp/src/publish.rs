//! Publishing a draft, and deleting a published article.
//!
//! - API (`freepublish/submit`, then polling `freepublish/get`): verified
//!   organization accounts only since July 2025.
//! - Backend, as the web page does it: the send page's context
//!   (`operation_seq`, protection status), the originality check, the
//!   admin's safety scan when the account requires one (a QR code shown here
//!   and confirmed on the admin's phone), then `masssend`. Without
//!   `--notify` this is 发表 (no push to followers); with it, 群发.

use std::time::Duration;

use social_core::output::note;
use social_core::{Action, Error, ErrorCode, Result, Value, ValueExt, json};

use crate::client::Client;
use crate::drafts::{self, is_web_id};
use crate::refs;
use crate::web::{MP, nested};

pub async fn publish(c: &Client, draft: &str, notify: bool) -> Result<Action> {
  if is_web_id(draft) {
    return web_publish(c, draft, notify).await;
  }
  if notify {
    return Err(Error::unsupported("publish --notify through the API").with_hint(
      "API mass sends need a verified account and the 群发 quota; publish a backend draft instead",
    ));
  }
  api_publish(c, draft).await
}

async fn api_publish(c: &Client, media_id: &str) -> Result<Action> {
  let v = c
    .api_post(
      "/cgi-bin/freepublish/submit",
      &json!({ "media_id": media_id }),
    )
    .await?;
  let publish_id = v
    .str("publish_id")
    .ok_or_else(|| Error::upstream("freepublish/submit returned no publish_id"))?;
  for _ in 0..30 {
    tokio::time::sleep(Duration::from_secs(3)).await;
    let s = c
      .api_post(
        "/cgi-bin/freepublish/get",
        &json!({ "publish_id": publish_id }),
      )
      .await?;
    match s.i64("publish_status") {
      Some(1) => continue,
      Some(0) => {
        let url = s.str("article_detail.item.0.article_url");
        let mut action = Action::done("publish", media_id);
        if let Some(id) = s.str("article_id") {
          action = action.with_id(id);
        }
        if let Some(u) = url {
          action = action.with_url(u);
        }
        return Ok(action);
      }
      Some(code) => {
        let why = match code {
          2 => "the originality check failed",
          3 => "publishing failed",
          4 => "the platform's review rejected it",
          5 => "it was deleted after publishing",
          6 => "it was banned after publishing",
          _ => "unknown status",
        };
        return Err(Error::upstream(format!(
          "publish {publish_id}: {why} (status {code}, failed articles {})",
          s.at("fail_idx")
        )));
      }
      None => return Err(Error::upstream(format!("unexpected publish status: {s}"))),
    }
  }
  Ok(
    Action::done("publish", media_id)
      .with_id(&publish_id)
      .with_message("still publishing; the article appears in `social mp user-posts` once done"),
  )
}

async fn web_publish(c: &Client, appmsgid: &str, notify: bool) -> Result<Action> {
  c.require_web()?;
  let multi = drafts::web_article_count(c, appmsgid).await? > 1;
  let page = c
    .web_get(
      "/cgi-bin/masssendpage",
      vec![("preview_appmsgid", appmsgid.into())],
    )
    .await?;
  let mut seq = page.str("operation_seq").unwrap_or_default();
  let protect = nested(&page, "strategy_info")
    .i64("protect_status")
    .unwrap_or(0);

  let list = copyright_check(c, appmsgid, &seq).await?;
  let code = if [0, 1, 4, 5, 8, 9, 12, 13].contains(&protect) {
    String::new()
  } else {
    let (code, new_seq) = safety_scan(c, appmsgid, notify).await?;
    if let Some(s) = new_seq {
      seq = s;
    }
    code
  };
  let ms = jiff::Timestamp::now().as_millisecond();
  let form = vec![
    ("random", format!("{}", rand::random::<f64>())),
    ("ack", String::new()),
    ("code", code),
    ("reprint_info", r#"{"item_list":[]}"#.into()),
    ("reprint_confirm", "1".into()),
    ("list", list),
    ("send_time", "0".into()),
    ("type", "10".into()),
    ("share_page", "1".into()),
    ("synctxweibo", "0".into()),
    ("operation_seq", seq),
    ("req_id", format!("{:032x}", rand::random::<u128>())),
    ("req_time", ms.to_string()),
    ("sync_version", "1".into()),
    ("isFreePublish", (!notify).to_string()),
    ("appmsgid", appmsgid.into()),
    ("isMulti", if multi { "1" } else { "0" }.into()),
    ("direct_send", "1".into()),
  ];
  let v = c
    .web_post_raw(
      "/cgi-bin/masssend",
      vec![
        ("t", "ajax-response".into()),
        (
          "is_release_publish_page",
          if notify { "0" } else { "1" }.into(),
        ),
      ],
      form,
    )
    .await?;
  let ret = v.i64("base_resp.ret").or_else(|| v.i64("ret")).unwrap_or(0);
  if ret != 0 {
    let why = match ret {
      720002 => "the admin's scan expired",
      64004 => "no publishing quota left today",
      154017 | 720005 => "the account has no admin bound for the safety check",
      154020 => "the account is banned from publishing",
      _ => "the backend refused it",
    };
    return Err(Error::upstream(format!(
      "publish failed: {why} (ret {ret})"
    )));
  }
  let mut action = Action::done(if notify { "mass-send" } else { "publish" }, appmsgid);
  if let Some(url) = find_published(c, appmsgid).await {
    action = action.with_url(url);
  } else {
    action = action.with_message("sent; the link appears in `social mp user-posts` shortly");
  }
  Ok(action)
}

/// The originality check before a send: `list` for the send form.
async fn copyright_check(c: &Client, appmsgid: &str, seq: &str) -> Result<String> {
  for round in 0..10 {
    let v = c
      .web_post_raw(
        "/cgi-bin/masssend",
        vec![("action", "get_appmsg_copyright_stat".into())],
        vec![
          ("first_check", if round == 0 { "1" } else { "0" }.into()),
          ("type", "10".into()),
          ("appmsgid", appmsgid.into()),
          ("operation_seq", seq.into()),
          ("reprint_info", String::new()),
          ("reprint_confirm", "0".into()),
        ],
      )
      .await?;
    match v.i64("base_resp.ret").or_else(|| v.i64("ret")).unwrap_or(0) {
      154011 => tokio::time::sleep(Duration::from_millis(1200)).await,
      154008 => return Ok(v.str("list").unwrap_or_default()),
      0 | 154009 => return Ok(String::new()),
      _ => {
        crate::web::check(v)?;
      }
    }
  }
  Err(Error::upstream(
    "the originality check did not finish; try again later",
  ))
}

/// The admin's safety scan: show the QR code and wait for the confirmation.
/// Returns the confirmation code and a refreshed `operation_seq`.
async fn safety_scan(c: &Client, appmsgid: &str, notify: bool) -> Result<(String, Option<String>)> {
  let t = c
    .web_post(
      "/misc/safeassistant",
      vec![("1", "1".into())],
      vec![("action", "get_ticket".into())],
    )
    .await?;
  let ticket = t
    .str("ticket")
    .ok_or_else(|| Error::upstream("the safety check gave no ticket"))?;
  let conn = c
    .web_post(
      "/safe/safeqrconnect",
      vec![("1", "1".into())],
      vec![
        ("state", "0".into()),
        ("login_type", "safe_center".into()),
        ("type", "json".into()),
        ("ticket", ticket.clone()),
      ],
    )
    .await?;
  let uuid = conn
    .str("uuid")
    .ok_or_else(|| Error::upstream("the safety check gave no uuid"))?;
  let mut q = vec![
    ("ticket", ticket),
    ("uuid", uuid.clone()),
    ("action", "check".to_owned()),
    ("service_type", "1".into()),
    ("type", "msgs".into()),
    ("msgid", appmsgid.into()),
  ];
  if !notify {
    q.push(("publish_type", "1".into()));
  }
  let image = c
    .ctx
    .http
    .get(format!("{MP}/safe/safeqrcode"))
    .queries(q)
    .send()
    .await?
    .check()?
    .body;
  let path = social_core::qr::show_image(&image, c.ctx.store.cache_dir())?;
  note(&format!(
    "The account admin must scan this with WeChat to allow publishing. QR image: {}",
    path.display()
  ));
  let started = std::time::Instant::now();
  while started.elapsed() < Duration::from_secs(180) {
    tokio::time::sleep(Duration::from_secs(2)).await;
    let v = c
      .web_post_raw(
        "/safe/safeuuid",
        vec![(
          "timespam",
          jiff::Timestamp::now().as_millisecond().to_string(),
        )],
        vec![
          ("uuid", uuid.clone()),
          ("action", "json".into()),
          ("type", "json".into()),
        ],
      )
      .await?;
    match v.i64("errcode") {
      Some(405) => return Ok((v.str("code").unwrap_or_default(), t.str("operation_seq"))),
      Some(403) => return Err(Error::auth("the admin cancelled the safety check")),
      _ => {}
    }
  }
  Err(Error::new(
    ErrorCode::VerificationRequired,
    "timed out waiting for the admin's safety scan",
  ))
}

/// The link of a just-published draft, from the newest published groups.
async fn find_published(c: &Client, appmsgid: &str) -> Option<String> {
  for _ in 0..6 {
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let v = c
      .web_get(
        "/cgi-bin/appmsgpublish",
        vec![
          ("sub", "list".into()),
          ("begin", "0".into()),
          ("count", "5".into()),
        ],
      )
      .await
      .ok()?;
    for g in nested(&v, "publish_page").list("publish_list") {
      let info = nested(g, "publish_info");
      if info.str("publish_info.draft_msgid").as_deref() == Some(appmsgid) {
        let first = info
          .list("appmsg_info")
          .first()
          .or_else(|| info.list("appmsgex").first())
          .cloned()
          .unwrap_or_default();
        return first.first_str(&["content_url", "link"]);
      }
    }
  }
  None
}

/// Delete a published article through the API (verified accounts): by
/// `ARTICLE_ID[/INDEX]` (as `user-posts` lists them over the API) or by its
/// link, looked up among the API's published list.
pub async fn delete(c: &Client, arg: &str) -> Result<Action> {
  let (article_id, index) = if arg.contains("mp.weixin.qq.com") {
    find_article_id(c, arg).await?
  } else {
    match arg.split_once('/') {
      Some((id, i)) => (id.to_owned(), i.parse().unwrap_or(0)),
      None => (arg.to_owned(), 0),
    }
  };
  c.api_post(
    "/cgi-bin/freepublish/delete",
    &json!({ "article_id": article_id, "index": index }),
  )
  .await?;
  Ok(Action::done("delete", &article_id))
}

async fn find_article_id(c: &Client, link: &str) -> Result<(String, u64)> {
  let target = article_key(link, c).await?;
  for page in 0..5 {
    let v = c
      .api_post(
        "/cgi-bin/freepublish/batchget",
        &json!({ "offset": page * 20, "count": 20, "no_content": 1 }),
      )
      .await?;
    let items = v.list("item");
    for item in items {
      for (i, n) in item.list("content.news_item").iter().enumerate() {
        let url = n.str("url").unwrap_or_default();
        if key_of(&url).as_ref() == Some(&target) {
          return Ok((item.str("article_id").unwrap_or_default(), i as u64 + 1));
        }
      }
    }
    if items.len() < 20 {
      break;
    }
  }
  Err(Error::not_found(
    "the article is not among the API's published posts (posts sent with notification are not listed there)",
  ))
}

/// `(mid, idx)` of an article, from its long link or its page.
async fn article_key(link: &str, c: &Client) -> Result<(String, String)> {
  if let Some(k) = key_of(link) {
    return Ok(k);
  }
  let post = crate::page::read(&c.ctx, link).await?;
  let get = |k: &str| post.extra.get(k).and_then(Value::as_str).map(str::to_owned);
  match (get("mid"), get("idx")) {
    (Some(m), Some(i)) => Ok((m, i)),
    _ => Err(Error::upstream("the article page names no mid / idx")),
  }
}

fn key_of(link: &str) -> Option<(String, String)> {
  let url = url::Url::parse(&refs::article_url(link).ok()?).ok()?;
  let get = |k: &str| {
    url
      .query_pairs()
      .find(|(n, _)| n == k)
      .map(|(_, v)| v.into_owned())
  };
  Some((get("mid")?, get("idx")?))
}

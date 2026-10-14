//! Someone's share (`pan.quark.cn/s/<id>`): what it holds, read with the
//! share token (`stoken`) its id and code give, and saving it into your
//! drive (转存), a background task. Files are never fetched from a share
//! directly: save it, then download from your drive.

use atp_core::{Action, Error, FileEntry, Page, PageReq, Result, ShareLink, Value, ValueExt, json};

use crate::api::Api;
use crate::refs::ShareRef;
use crate::{files, parse, paths, tasks};

const PAGE: usize = 100;
/// Pages of a share's top level that `save` reads.
const MAX_PAGES: u64 = 20;

/// The share itself: title, size, top-level entries (`files`), counters,
/// its end.
/// The page describes it the way `shares` describes your own.
pub async fn read(api: &Api, share: &ShareRef) -> Result<FileEntry> {
  let stoken = token(api, share).await?;
  let v = detail(api, share, &stoken, "0", 1, 50).await?;
  let info = v.at("data.share");
  let mut entry = parse::share(info).unwrap_or_default();
  if entry.name.is_empty() {
    let first = v.list("data.list").first().and_then(|f| f.str("file_name"));
    entry.name = parse::unescape(&first.unwrap_or_else(|| share.id.clone()));
  }
  entry.id = share.id.clone();
  entry.dir = true;
  entry.size = info.u64("size").filter(|s| *s > 0);
  entry.share = Some(ShareLink {
    url: share.url(),
    code: share.code.clone(),
    expires_at: parse::link(info).expires_at,
  });
  if v.bool("data.is_owner") == Some(true) {
    entry.extra.insert("yours".into(), json!(true));
  }
  entry.raw = Some(v.at("data").clone());
  Ok(entry)
}

/// The entries of a share's folder (`folder` a `fid` from an earlier
/// listing; the top level without it).
pub async fn list(
  api: &Api,
  share: &ShareRef,
  folder: Option<&str>,
  req: &PageReq,
) -> Result<Page<FileEntry>> {
  let stoken = token(api, share).await?;
  let (page, size) = (req.number_or(1), req.size_within(PAGE));
  let v = detail(api, share, &stoken, folder.unwrap_or("0"), page, size).await?;
  let link = ShareLink {
    url: share.url(),
    code: share.code.clone(),
    expires_at: None,
  };
  let items = v
    .list("data.list")
    .iter()
    .filter_map(|item| parse::file(item, None))
    .map(|mut entry| {
      entry.share = Some(link.clone());
      entry
    })
    .collect();
  Ok(files::paged(items, &v, page, size))
}

/// Save everything a share holds into `to` (a folder of your drive; the
/// root without it).
pub async fn save(api: &Api, share: &ShareRef, to: Option<&str>) -> Result<Action> {
  let dest = match to {
    Some(arg) => files::folder(api, arg).await?,
    None => paths::root(),
  };
  let stoken = token(api, share).await?;
  let (fids, tokens) = top_level(api, share, &stoken).await?;
  let count = fids.len();
  let body = json!({
    "fid_list": fids,
    "fid_token_list": tokens,
    "to_pdir_fid": dest.id,
    "pwd_id": share.id,
    "stoken": stoken,
    "pdir_fid": "0",
    "scene": "link",
  });
  tasks::settle(api, &api.post("/share/sharepage/save", &body).await?).await?;
  paths::forget(api);
  Ok(
    Action::done("save", share.url())
      .with_message(format!("saved {count} entries into {}", dest.label())),
  )
}

/// Ids and save tokens of everything at a share's top level.
async fn top_level(
  api: &Api,
  share: &ShareRef,
  stoken: &str,
) -> Result<(Vec<String>, Vec<String>)> {
  let (mut fids, mut tokens) = (Vec::new(), Vec::new());
  for page in 1..=MAX_PAGES {
    let v = detail(api, share, stoken, "0", page, PAGE).await?;
    if v.bool("data.is_owner") == Some(true) {
      return Err(Error::input(
        "this is your own share: its files are in your drive already",
      ));
    }
    for item in v.list("data.list") {
      if let (Some(fid), Some(token)) = (item.str("fid"), item.str("share_fid_token")) {
        fids.push(fid);
        tokens.push(token);
      }
    }
    if !files::paged(Vec::<()>::new(), &v, page, PAGE).has_more {
      break;
    }
  }
  if fids.is_empty() {
    return Err(Error::not_found(format!(
      "{} holds nothing to save",
      share.url()
    )));
  }
  Ok((fids, tokens))
}

/// The share token for a link and its code.
async fn token(api: &Api, share: &ShareRef) -> Result<String> {
  let body = json!({
    "pwd_id": share.id,
    "passcode": share.code.clone().unwrap_or_default(),
    "support_visit_limit_private_share": true,
  });
  let v = api.post("/share/sharepage/token", &body).await?;
  v.str("data.stoken")
    .ok_or_else(|| Error::upstream("Quark gave no token for this share"))
}

async fn detail(
  api: &Api,
  share: &ShareRef,
  stoken: &str,
  folder: &str,
  page: u64,
  size: usize,
) -> Result<Value> {
  let query = [
    ("pwd_id", share.id.clone()),
    ("stoken", stoken.to_owned()),
    ("pdir_fid", folder.to_owned()),
    ("force", "0".into()),
    ("_page", page.to_string()),
    ("_size", size.to_string()),
    ("_fetch_banner", "0".into()),
    ("_fetch_share", "1".into()),
    ("_fetch_total", "1".into()),
    ("_sort", "file_type:asc,file_name:asc".into()),
  ];
  api.get("/share/sharepage/detail", &query).await
}

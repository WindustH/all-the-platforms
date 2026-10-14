//! Someone's share (`pan.baidu.com/s/1<key>`, web drive only): what it
//! holds, and saving it into your drive (转存). A share with a code is
//! opened first (`/share/verify`); the key it answers (`randsk`) goes along
//! as the `BDCLND` cookie, for this run only. Files are never fetched out
//! of a share: save it, then download from your drive.

use atp_core::http::Method;
use atp_core::{
  Action, Error, ErrorCode, FileEntry, Page, PageReq, Result, ShareLink, Value, ValueExt, json,
};

use crate::api::{Api, Call};
use crate::files::{self, numbered};
use crate::refs::{self, ShareRef};
use crate::{parse, shares, tasks};

const PAGE: usize = 100;
/// Pages of a share's top level that `save` reads.
const MAX_PAGES: u64 = 20;

/// A share opened for listing: the cookie its code unlocked, and for a
/// folder inside it, the ids of the share and its owner that listing one
/// takes.
pub struct Opened {
  share: ShareRef,
  cookie: String,
  dir: Option<Folder>,
}

struct Folder {
  path: String,
  uk: String,
  share_id: String,
}

/// Open `share` (its code checked), to list its top level or `dir`, a
/// folder path from an earlier `share-ls`.
pub async fn open(api: &Api, share: &ShareRef, dir: Option<&str>) -> Result<Opened> {
  api.require_web("reading a share")?;
  let mut opened = Opened {
    share: share.clone(),
    cookie: unlock(api, share).await?,
    dir: None,
  };
  if let Some(dir) = dir {
    let top = listing(api, &opened, 1, 1).await?;
    let (Some(uk), Some(share_id)) = (top.str("uk"), top.str("share_id")) else {
      return Err(Error::upstream("Baidu did not say whose share this is"));
    };
    opened.dir = Some(Folder {
      path: dir.to_owned(),
      uk,
      share_id,
    });
  }
  Ok(opened)
}

/// One page of an opened share.
pub async fn page(api: &Api, opened: &Opened, req: &PageReq) -> Result<Page<FileEntry>> {
  let page = req.number_or(1);
  let v = listing(api, opened, page, PAGE).await?;
  let link = ShareLink {
    url: opened.share.url(),
    code: opened.share.code.clone(),
    expires_at: None,
  };
  let items = v
    .list("list")
    .iter()
    .filter_map(parse::file)
    .map(|mut entry| {
      entry.share = Some(link.clone());
      entry
    })
    .collect();
  Ok(numbered(items, page, PAGE))
}

/// The share itself: its title, its link and code, how many entries its
/// top level holds.
pub async fn read(api: &Api, share: &ShareRef) -> Result<FileEntry> {
  let opened = open(api, share, None).await?;
  let v = listing(api, &opened, 1, PAGE).await?;
  let entries = v.list("list");
  let name = v
    .str("title")
    .map(|t| refs::split(&t).1)
    .filter(|t| !t.is_empty())
    .or_else(|| entries.first().and_then(|e| e.str("server_filename")))
    .unwrap_or_else(|| share.surl.clone());
  let mut entry = FileEntry {
    id: share.url(),
    name,
    dir: true,
    share: Some(ShareLink {
      url: share.url(),
      code: share.code.clone(),
      expires_at: None,
    }),
    raw: Some(v.clone()),
    ..FileEntry::default()
  };
  let count = if entries.len() >= PAGE {
    format!("{PAGE}+")
  } else {
    entries.len().to_string()
  };
  entry.extra.insert("entries".into(), json!(count));
  Ok(entry)
}

/// Save everything a share holds into `to` (a folder of your drive; the
/// root without it).
pub async fn save(api: &Api, share: &ShareRef, to: Option<&str>) -> Result<Action> {
  shares::require(api, "saving a share")?;
  let dest = match to {
    Some(arg) => files::path_of(api, &api.ctx.post_ref(arg)?).await?,
    None => "/".to_owned(),
  };
  let opened = open(api, share, None).await?;
  let (fs_ids, uk, share_id) = top_level(api, &opened).await?;
  let call = Call {
    query: vec![
      ("shareid", share_id),
      ("from", uk),
      ("bdstoken", api.bdstoken().await?),
      ("channel", "chunlei".into()),
    ],
    form: Some(vec![
      ("fsidlist", format!("[{}]", fs_ids.join(","))),
      ("path", dest.clone()),
    ]),
    referer: Some(share.url()),
    cookie: Some(opened.cookie.clone()),
    ..Call::default()
  };
  let v = api
    .call(Method::POST, "/share/transfer", call)
    .await
    .map_err(refused)?;
  tasks::settle(api, &v).await?;
  Ok(
    Action::done("save", share.url())
      .with_message(format!("saved {} entries into {dest}", fs_ids.len())),
  )
}

/// File ids at a share's top level, and the ids of its owner and itself.
async fn top_level(api: &Api, opened: &Opened) -> Result<(Vec<String>, String, String)> {
  let mut fs_ids = Vec::new();
  let mut owner = None;
  for page in 1..=MAX_PAGES {
    let v = listing(api, opened, page, PAGE).await?;
    if owner.is_none() {
      owner = v.str("uk").zip(v.str("share_id"));
    }
    let entries = v.list("list");
    fs_ids.extend(entries.iter().filter_map(|e| e.str("fs_id")));
    if entries.len() < PAGE {
      break;
    }
  }
  let Some((uk, share_id)) = owner else {
    return Err(Error::upstream("Baidu did not say whose share this is"));
  };
  if fs_ids.is_empty() {
    return Err(Error::not_found(format!(
      "{} holds nothing to save",
      opened.share.url()
    )));
  }
  Ok((fs_ids, uk, share_id))
}

/// Check the share's code; the cookie that carries the key it unlocks
/// (empty for a share without a code).
async fn unlock(api: &Api, share: &ShareRef) -> Result<String> {
  let Some(code) = share.code.clone() else {
    return Ok(String::new());
  };
  let call = Call {
    query: vec![
      ("surl", share.surl.clone()),
      ("t", jiff::Timestamp::now().as_millisecond().to_string()),
      ("channel", "chunlei".into()),
      ("bdstoken", api.bdstoken().await?),
    ],
    form: Some(vec![
      ("pwd", code),
      ("vcode", String::new()),
      ("vcode_str", String::new()),
    ]),
    referer: Some(share.init_url()),
    cookie: Some(String::new()),
    ..Call::default()
  };
  let v = api
    .call(Method::POST, "/share/verify", call)
    .await
    .map_err(refused)?;
  let key = v
    .str("randsk")
    .ok_or_else(|| Error::upstream("Baidu took the code but gave no key"))?;
  Ok(format!("BDCLND={key}"))
}

/// One page of the share's top level, or of the folder it was opened at.
async fn listing(api: &Api, opened: &Opened, page: u64, size: usize) -> Result<Value> {
  let mut query = vec![
    ("page", page.to_string()),
    ("num", size.to_string()),
    ("order", "name".into()),
    ("desc", "0".into()),
    ("showempty", "0".into()),
    ("view_mode", "1".into()),
    ("channel", "chunlei".into()),
  ];
  match &opened.dir {
    None => query.extend([
      ("shorturl", opened.share.surl.clone()),
      ("root", "1".into()),
    ]),
    Some(folder) => query.extend([
      ("uk", folder.uk.clone()),
      ("shareid", folder.share_id.clone()),
      ("dir", folder.path.clone()),
    ]),
  }
  let call = Call {
    query,
    referer: Some(opened.share.url()),
    cookie: Some(opened.cookie.clone()),
    ..Call::default()
  };
  api
    .call(Method::GET, "/share/list", call)
    .await
    .map_err(refused)
}

/// What a share's refusal means for the user.
fn refused(e: Error) -> Error {
  let code = |c: i64| e.message.ends_with(&format!("(errno {c})"));
  if code(-9) || code(-12) {
    Error::input(e.message).with_hint("the extraction code is wrong or missing: pass it with --pwd")
  } else if code(-7) || code(105) || code(-8) || code(115) {
    Error::new(ErrorCode::NotFound, e.message)
      .with_hint("the share was cancelled, expired or taken down")
  } else {
    e
  }
}

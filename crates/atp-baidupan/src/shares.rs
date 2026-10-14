//! Your shares (web drive only): making one (always with an extraction
//! code, which Baidu asks for), listing them, and cancelling them. Shares
//! need the drive's `STOKEN` cookie besides `BDUSS`.

use atp_core::cli::confirm;
use atp_core::http::Method;
use atp_core::{Action, Error, FileEntry, Page, PageReq, Result, ShareLink, ValueExt, json};
use rand::Rng;

use crate::api::{Api, Call, WEB_HINT};
use crate::{files, parse, refs};

const HOME: &str = "https://pan.baidu.com/disk/home";
const PAGE: usize = 50;
const CODE_CHARS: &[u8] = b"abcdefghijkmnpqrstuvwxyz23456789";

/// Fail unless the cookies shares need are there.
pub fn require(api: &Api, what: &str) -> Result<()> {
  api.require_web(what)?;
  if api.ctx.http.has_cookie("STOKEN") {
    return Ok(());
  }
  Err(
    Error::auth(format!(
      "{what} needs the drive's STOKEN cookie, which this login lacks"
    ))
    .with_hint(WEB_HINT),
  )
}

/// Share files and folders in one link that works `days` days (0: no end),
/// with `code` as its extraction code (a random one without it).
pub async fn share(
  api: &Api,
  args: &[String],
  days: u32,
  code: Option<String>,
) -> Result<FileEntry> {
  require(api, "sharing")?;
  if ![0, 1, 7, 30].contains(&days) {
    return Err(Error::input("--expire takes 0 (never), 1, 7 or 30 days"));
  }
  let code = code_or_random(code)?;
  let mut paths = Vec::new();
  for arg in args {
    paths.push(files::changeable(api, arg).await?);
  }
  let Some(first) = paths.first().cloned() else {
    return Err(Error::input("expected something to share"));
  };
  let call = Call {
    query: vec![
      ("bdstoken", api.bdstoken().await?),
      ("channel", "chunlei".into()),
    ],
    form: Some(vec![
      ("path_list", json!(paths).to_string()),
      ("period", days.to_string()),
      ("pwd", code.clone()),
      ("schannel", "4".into()),
      ("channel_list", "[]".into()),
      ("share_type", "9".into()),
    ]),
    referer: Some(HOME.into()),
    ..Call::default()
  };
  let v = api.call(Method::POST, "/share/pset", call).await?;
  let id = v
    .str("shareid")
    .ok_or_else(|| Error::upstream("Baidu made the share but named no share id"))?;
  let url = v
    .first_str(&["link", "shorturl"])
    .ok_or_else(|| Error::upstream("Baidu made the share but gave no link"))?;
  let mut entry = FileEntry {
    id,
    name: refs::split(&first).1,
    path: Some(first),
    dir: paths.len() > 1,
    share: Some(ShareLink {
      url,
      code: Some(code),
      expires_at: expiry(days),
    }),
    raw: Some(v.clone()),
    ..FileEntry::default()
  };
  entry.extra.insert("files".into(), json!(paths.len()));
  Ok(entry)
}

/// When a share made now for `days` days stops working.
fn expiry(days: u32) -> Option<jiff::Timestamp> {
  let secs = i64::from(days) * 86_400;
  (days > 0)
    .then(|| jiff::Timestamp::now().checked_add(jiff::SignedDuration::from_secs(secs)))
    .and_then(Result::ok)
}

/// One page of your shares, newest first. The cursor is `page.seen`: how
/// many came before, as Baidu only tells the total.
pub async fn list(api: &Api, req: &PageReq) -> Result<Page<FileEntry>> {
  api.require_web("listing your shares")?;
  let (page, seen) = position(req);
  let call = Call {
    query: vec![
      ("page", page.to_string()),
      ("num", PAGE.to_string()),
      ("desc", "1".into()),
      ("order", "time".into()),
    ],
    referer: Some(HOME.into()),
    ..Call::default()
  };
  let v = api.call(Method::GET, "/share/record", call).await?;
  let items: Vec<FileEntry> = v.list("list").iter().filter_map(parse::share).collect();
  let seen = seen + items.len() as u64;
  let more = match v.u64("total") {
    Some(total) => seen < total && !items.is_empty(),
    None => items.len() >= PAGE,
  };
  Ok(Page::new(
    items,
    more.then(|| format!("{}.{seen}", page + 1)),
  ))
}

fn position(req: &PageReq) -> (u64, u64) {
  let cursor = req.cursor.as_deref().unwrap_or("1.0");
  let (page, seen) = cursor.split_once('.').unwrap_or((cursor, "0"));
  (
    page.parse().unwrap_or(1).max(1),
    seen.parse().unwrap_or_default(),
  )
}

/// Cancel shares by their ids (from `shares`).
pub async fn unshare(api: &Api, ids: &[String], yes: bool) -> Result<Action> {
  api.require_web("cancelling shares")?;
  let mut numbers = Vec::new();
  for id in ids {
    let id = api.ctx.post_ref(id)?;
    let number: u64 = id
      .parse()
      .map_err(|_| Error::input(format!("`{id}` is not a share id from `shares`")))?;
    numbers.push(number);
  }
  confirm(
    yes,
    &format!(
      "cancel {} share(s); their links stop working",
      numbers.len()
    ),
  )?;
  let call = Call {
    query: vec![
      ("bdstoken", api.bdstoken().await?),
      ("channel", "chunlei".into()),
    ],
    form: Some(vec![("shareid_list", json!(numbers).to_string())]),
    referer: Some(HOME.into()),
    ..Call::default()
  };
  api.call(Method::POST, "/share/cancel", call).await?;
  let ids: Vec<String> = numbers.iter().map(u64::to_string).collect();
  Ok(Action::done("unshare", ids.join(", ")))
}

/// The code given, checked, or a random one.
fn code_or_random(code: Option<String>) -> Result<String> {
  let Some(code) = code.map(|c| c.trim().to_owned()).filter(|c| !c.is_empty()) else {
    return Ok(random_code());
  };
  if code.len() == 4 && code.bytes().all(|b| b.is_ascii_alphanumeric()) {
    return Ok(code);
  }
  Err(Error::input(
    "an extraction code is 4 letters or digits (--pwd abcd)",
  ))
}

fn random_code() -> String {
  let mut rng = rand::rng();
  (0..4)
    .map(|_| CODE_CHARS[rng.random_range(0..CODE_CHARS.len())] as char)
    .collect()
}

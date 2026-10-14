//! Your shares: making one (a background task, then its link and code),
//! listing them, and cancelling them.

use atp_core::cli::confirm;
use atp_core::{Action, Error, FileEntry, Page, PageReq, Result, ValueExt, json};
use rand::Rng;

use crate::api::Api;
use crate::{files, parse, tasks};

const PAGE: usize = 50;
const CODE_CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";

/// How long a new share works: days (0 for no end).
pub fn expiry(days: u32) -> Result<u32> {
  match days {
    0 => Ok(1),
    1 => Ok(2),
    7 => Ok(3),
    30 => Ok(4),
    _ => Err(Error::input("--expire takes 0 (never), 1, 7 or 30 days")),
  }
}

/// Share files and folders in one link; `code` asks for an extraction code
/// (a random one when empty).
pub async fn share(
  api: &Api,
  args: &[String],
  days: u32,
  code: Option<String>,
) -> Result<FileEntry> {
  let expired_type = expiry(days)?;
  let mut entries = Vec::new();
  for arg in args {
    entries.push(files::own(api, arg).await?);
  }
  let Some(first) = entries.first() else {
    return Err(Error::input("expected something to share"));
  };
  let mut body = json!({
    "fid_list": entries.iter().map(|e| e.id.clone()).collect::<Vec<_>>(),
    "title": first.name,
    "url_type": if code.is_some() { 2 } else { 1 },
    "expired_type": expired_type,
  });
  if let Some(code) = code {
    let code = if code.trim().is_empty() {
      random_code()
    } else {
      code.trim().to_owned()
    };
    body["passcode"] = json!(code);
  }
  let done = tasks::settle(api, &api.post("/share", &body).await?).await?;
  let id = done
    .str("share_id")
    .ok_or_else(|| Error::upstream("Quark made the share but named no share id"))?;
  let v = api
    .post("/share/password", &json!({ "share_id": id }))
    .await?;
  // The answer has the link, not the share id.
  let mut data = v.at("data").clone();
  if data.is_object() && data.get("share_id").is_none() {
    data["share_id"] = json!(id);
  }
  let mut entry =
    parse::share(&data).ok_or_else(|| Error::upstream("Quark made the share but gave no link"))?;
  entry.dir = first.dir || entries.len() > 1;
  Ok(entry)
}

/// One page of your shares, newest first.
pub async fn list(api: &Api, req: &PageReq) -> Result<Page<FileEntry>> {
  let (page, size) = (req.number_or(1), req.size_within(PAGE));
  let query = [
    ("_page", page.to_string()),
    ("_size", size.to_string()),
    ("_order_field", "created_at".into()),
    ("_order_type", "desc".into()),
    ("_fetch_total", "1".into()),
    ("_fetch_notify_follow", "1".into()),
  ];
  let v = api.get("/share/mypage/detail", &query).await?;
  let items = v
    .list("data.list")
    .iter()
    .filter_map(parse::share)
    .collect();
  Ok(files::paged(items, &v, page, size))
}

/// Cancel shares by their ids (from `shares`).
pub async fn unshare(api: &Api, ids: &[String], yes: bool) -> Result<Action> {
  let ids = ids
    .iter()
    .map(|id| api.ctx.post_ref(id))
    .collect::<Result<Vec<_>>>()?;
  confirm(
    yes,
    &format!("cancel {} share(s); their links stop working", ids.len()),
  )?;
  api
    .post("/share/delete", &json!({ "share_ids": ids }))
    .await?;
  Ok(Action::done("unshare", ids.join(", ")))
}

fn random_code() -> String {
  let mut rng = rand::rng();
  (0..4)
    .map(|_| CODE_CHARS[rng.random_range(0..CODE_CHARS.len())] as char)
    .collect()
}

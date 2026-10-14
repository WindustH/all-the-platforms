//! Offline downloads (离线下载, web drive only): Baidu's servers fetch a
//! link (http, https, ftp, magnet, ed2k) into your drive; atp hands the
//! link over and lists or cancels the tasks (`/rest/2.0/services/cloud_dl`).
//! How many tasks run at once depends on the membership; Baidu refuses the
//! ones over it.

use atp_core::cli::confirm;
use atp_core::{Action, Error, FileEntry, Page, PageReq, Result, ValueExt};

use crate::api::Api;
use crate::{files, parse};

const CLOUD_DL: &str = "/rest/2.0/services/cloud_dl";
const PAGE: usize = 100;

/// Hand links to Baidu to fetch into `to` (the root without it).
pub async fn add(api: &Api, urls: &[String], to: Option<&str>) -> Result<Action> {
  api.require_web("offline downloads")?;
  let dest = match to {
    Some(arg) => files::path_of(api, &api.ctx.post_ref(arg)?).await?,
    None => "/".to_owned(),
  };
  let mut ids = Vec::new();
  for url in urls {
    let url = url.trim();
    let selected = if url.starts_with("magnet:") {
      magnet_files(api, url, &dest).await?
    } else {
      String::new()
    };
    let query = [
      ("method", "add_task".to_owned()),
      ("task_from", "0".into()),
      ("source_url", url.to_owned()),
      ("save_path", dest.clone()),
      ("selected_idx", selected),
    ];
    let v = api.post(CLOUD_DL, &query, &[]).await?;
    let id = v
      .str("task_id")
      .ok_or_else(|| Error::upstream(format!("Baidu took {url} but named no task")))?;
    ids.push(id);
  }
  Ok(
    Action::done("offline add", urls.join(", "))
      .with_id(ids.join(","))
      .with_message(format!(
        "Baidu fetches it into {dest}; `offline list` shows how far it got"
      )),
  )
}

/// Every file of a magnet link, as the indices `add_task` selects.
async fn magnet_files(api: &Api, url: &str, dest: &str) -> Result<String> {
  let query = [
    ("method", "query_magnetinfo".to_owned()),
    ("source_url", url.to_owned()),
    ("save_path", dest.to_owned()),
    ("type", "4".into()),
  ];
  let v = api.post(CLOUD_DL, &query, &[]).await?;
  let count = v.list("magnet_info").len();
  if count == 0 {
    return Err(Error::not_found(format!(
      "Baidu found no files behind {url}"
    )));
  }
  Ok(
    (1..=count)
      .map(|i| i.to_string())
      .collect::<Vec<_>>()
      .join(","),
  )
}

/// One page of the offline tasks, newest first; the cursor is an offset.
pub async fn list(api: &Api, req: &PageReq) -> Result<Page<FileEntry>> {
  api.require_web("offline downloads")?;
  let start = req.number_or(0);
  let query = [
    ("method", "list_task".to_owned()),
    ("need_task_info", "1".into()),
    ("status", "255".into()),
    ("start", start.to_string()),
    ("limit", PAGE.to_string()),
  ];
  let v = api.post(CLOUD_DL, &query, &[]).await?;
  let items: Vec<FileEntry> = v.list("task_info").iter().filter_map(parse::task).collect();
  let next = start + items.len() as u64;
  let more = match v.u64("total") {
    Some(total) => next < total && !items.is_empty(),
    None => items.len() >= PAGE,
  };
  Ok(Page::new(items, more.then(|| next.to_string())))
}

/// Cancel tasks by their ids (from `offline list`).
pub async fn cancel(api: &Api, ids: &[String], yes: bool) -> Result<Action> {
  api.require_web("offline downloads")?;
  let mut tasks = Vec::new();
  for id in ids {
    let id = api.ctx.post_ref(id)?;
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
      return Err(Error::input(format!(
        "`{id}` is not a task id from `offline list`"
      )));
    }
    tasks.push(id);
  }
  confirm(yes, &format!("cancel {} offline download(s)", tasks.len()))?;
  for id in &tasks {
    let query = [
      ("method", "cancel_task".to_owned()),
      ("task_id", id.clone()),
    ];
    api.post(CLOUD_DL, &query, &[]).await?;
  }
  Ok(Action::done("offline cancel", tasks.join(", ")))
}

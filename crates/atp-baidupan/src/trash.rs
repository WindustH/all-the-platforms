//! The recycle bin (web drive only): what `delete` put there, restoring it,
//! and deleting it for good. Entries are known by their file id (`fs_id`).
//! Restores go to PCS (`pcs.baidu.com`), the way the web drive's own
//! clients do them.

use atp_core::cli::confirm;
use atp_core::http::Method;
use atp_core::{Action, Error, FileEntry, Page, PageReq, Result, ValueExt, json};

use crate::api::{Api, Call};
use crate::files::numbered;
use crate::parse;

const PAGE: usize = 100;
const PCS_FILE: &str = "https://pcs.baidu.com/rest/2.0/pcs/file";

/// One page of the recycle bin.
pub async fn list(api: &Api, req: &PageReq) -> Result<Page<FileEntry>> {
  api.require_web("the recycle bin")?;
  let page = req.number_or(1);
  let query = [("num", PAGE.to_string()), ("page", page.to_string())];
  let v = api.get("/api/recycle/list", &query).await?;
  let items = v.list("list").iter().filter_map(parse::trashed).collect();
  Ok(numbered(items, page, PAGE))
}

/// Put entries back where they were.
pub async fn restore(api: &Api, ids: &[String]) -> Result<Action> {
  api.require_web("restoring from the recycle bin")?;
  let ids = resolved(api, ids)?;
  let list: Vec<_> = ids
    .iter()
    .map(|id| json!({ "fs_id": id.parse::<u64>().unwrap_or_default() }))
    .collect();
  let call = Call {
    query: vec![("method", "restore".into())],
    form: Some(vec![("param", json!({ "list": list }).to_string())]),
    multipart: true,
    ..Call::default()
  };
  api.call(Method::POST, PCS_FILE, call).await?;
  Ok(Action::done("restore", ids.join(", ")))
}

/// Delete entries of the recycle bin for good.
pub async fn purge(api: &Api, ids: &[String], yes: bool) -> Result<Action> {
  api.require_web("purging the recycle bin")?;
  let ids = resolved(api, ids)?;
  confirm(
    yes,
    &format!("delete {} entries of the recycle bin for good", ids.len()),
  )?;
  let query = [("bdstoken", api.bdstoken().await?)];
  let form = [("fidlist", format!("[{}]", ids.join(",")))];
  api.post("/api/recycle/delete", &query, &form).await?;
  Ok(Action::done("purge", ids.join(", ")))
}

/// File ids, `#N` from the last `trash` listing included.
fn resolved(api: &Api, ids: &[String]) -> Result<Vec<String>> {
  ids
    .iter()
    .map(|id| {
      let id = api.ctx.post_ref(id)?;
      if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Error::input(format!(
          "`{id}` is not a file id from `trash`"
        )));
      }
      Ok(id)
    })
    .collect()
}

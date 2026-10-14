//! The recycle bin: what `delete` put there, restoring it, and deleting it
//! for good. Entries are known by their record id (`record_id`).

use atp_core::cli::confirm;
use atp_core::{Action, FileEntry, Page, PageReq, Result, ValueExt, json};

use crate::api::Api;
use crate::{files, parse, paths, tasks};

const PAGE: usize = 100;

/// One page of the recycle bin, most recently deleted first.
pub async fn list(api: &Api, req: &PageReq) -> Result<Page<FileEntry>> {
  let (page, size) = (req.number_or(1), req.size_within(PAGE));
  let query = [
    ("_page", page.to_string()),
    ("_size", size.to_string()),
    ("_fetch_total", "1".into()),
    ("fetch_pdir_file_name", "1".into()),
    ("sort", "move_recycle_at:desc".into()),
  ];
  let v = api.get("/file/recycle/list", &query).await?;
  let items = v
    .list("data.list")
    .iter()
    .filter_map(parse::trashed)
    .collect();
  Ok(files::paged(items, &v, page, size))
}

/// Put records back where they were.
pub async fn restore(api: &Api, records: &[String]) -> Result<Action> {
  let records = resolved(api, records)?;
  let body = json!({ "record_list": records, "select_mode": 2 });
  tasks::settle(api, &api.post("/file/recycle/recover", &body).await?).await?;
  paths::forget(api);
  Ok(Action::done("restore", records.join(", ")))
}

/// Delete records for good.
pub async fn purge(api: &Api, records: &[String], yes: bool) -> Result<Action> {
  let records = resolved(api, records)?;
  confirm(
    yes,
    &format!(
      "delete {} entries of the recycle bin for good",
      records.len()
    ),
  )?;
  let body = json!({ "record_list": records, "select_mode": 2 });
  tasks::settle(api, &api.post("/file/recycle/remove", &body).await?).await?;
  Ok(Action::done("purge", records.join(", ")))
}

/// Record ids, `#N` from the last `trash` listing included.
fn resolved(api: &Api, records: &[String]) -> Result<Vec<String>> {
  records.iter().map(|r| api.ctx.post_ref(r)).collect()
}

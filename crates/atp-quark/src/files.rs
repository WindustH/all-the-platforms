//! Files and folders of the drive: one entry, a folder's entries, search
//! by name, and the changes (folders, renames, moves, copies, deletes).

use atp_core::{Action, Data, Error, FileEntry, Page, PageReq, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::refs::{self, FileRef};
use crate::{parse, paths, sharepages, tasks};

/// Folders first, newest first: the web drive's order.
const DEFAULT_SORT: &str = "file_type:asc,updated_at:desc";
const PAGE: usize = 100;

/// The order of a listing: `name` or `time`, descending or not.
#[derive(Debug, Clone, Copy)]
pub struct Sort<'a> {
  pub by: &'a str,
  pub desc: bool,
}

impl Sort<'_> {
  fn param(&self) -> String {
    let field = if self.by == "name" {
      "file_name"
    } else {
      "updated_at"
    };
    let order = if self.desc { "desc" } else { "asc" };
    format!("file_type:asc,{field}:{order}")
  }
}

/// A file id, a path or a share link.
pub async fn file(api: &Api, arg: &str) -> Result<FileEntry> {
  match refs::file(arg)? {
    FileRef::Id(fid) => info(api, &fid).await,
    FileRef::Path(path) => paths::resolve(api, &path).await,
    FileRef::Share(share) => sharepages::read(api, &share).await,
  }
}

/// A file id or path of your own drive (not a share, not the root).
pub async fn own(api: &Api, arg: &str) -> Result<FileEntry> {
  let entry = file(api, &api.ctx.post_ref(arg)?).await?;
  if entry.share.is_some() {
    return Err(Error::input(format!(
      "{arg} is a share; `save` it into your drive first"
    )));
  }
  if entry.id == "0" {
    return Err(Error::input("the root folder cannot be changed"));
  }
  Ok(entry)
}

/// A folder of your own drive, by id or path (`/` for the root).
pub async fn folder(api: &Api, arg: &str) -> Result<FileEntry> {
  let entry = file(api, &api.ctx.post_ref(arg)?).await?;
  if !entry.dir || entry.share.is_some() {
    return Err(Error::input(format!("{arg} is not a folder of your drive")));
  }
  Ok(entry)
}

pub async fn info(api: &Api, fid: &str) -> Result<FileEntry> {
  let query = [
    ("fid", fid.to_owned()),
    ("_fetch_full_path", "1".into()),
    ("fetch_risk_file_name", "1".into()),
  ];
  let v = api.get("/file/info", &query).await?;
  parse::file(v.at("data"), None)
    .ok_or_else(|| Error::not_found(format!("no file {fid} in your drive")))
}

/// One page of a folder's entries.
pub async fn list(
  api: &Api,
  folder: &FileEntry,
  sort: Option<Sort<'_>>,
  req: &PageReq,
) -> Result<Page<FileEntry>> {
  let (page, size) = (req.number_or(1), req.size_within(PAGE));
  let sort = sort.map_or_else(|| DEFAULT_SORT.to_owned(), |s| s.param());
  let query = [
    ("pdir_fid", folder.id.clone()),
    ("_page", page.to_string()),
    ("_size", size.to_string()),
    ("_fetch_total", "1".into()),
    ("_fetch_sub_dirs", "0".into()),
    ("_sort", sort),
    ("fetch_all_file", "1".into()),
    ("fetch_risk_file_name", "1".into()),
  ];
  let v = api.get("/file/sort", &query).await?;
  let parent = folder.path.as_deref();
  let items = v
    .list("data.list")
    .iter()
    .filter_map(|item| parse::file(item, parent))
    .collect();
  Ok(paged(items, &v, page, size))
}

/// Files and folders of the whole drive whose names contain `query`.
pub async fn search(api: &Api, query: &str, req: &PageReq) -> Result<Page<FileEntry>> {
  let (page, size) = (req.number_or(1), req.size_within(PAGE));
  let params = [
    ("q", query.to_owned()),
    ("_page", page.to_string()),
    ("_size", size.to_string()),
    ("_fetch_total", "1".into()),
    ("_sort", DEFAULT_SORT.into()),
  ];
  let v = api.get("/file/search", &params).await?;
  let items = v
    .list("data.list")
    .iter()
    .filter_map(|item| parse::file(item, None))
    .collect();
  Ok(paged(items, &v, page, size))
}

/// A page numbered from 1, continued while `metadata._total` says so.
pub fn paged<T>(items: Vec<T>, v: &Value, page: u64, size: usize) -> Page<T> {
  let more = match v.u64("metadata._total") {
    Some(total) => page * (size as u64) < total,
    None => items.len() >= size,
  };
  Page::new(items, more.then(|| (page + 1).to_string()))
}

/// Move a file or folder to the recycle bin.
pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  let entry = own(api, arg).await?;
  let body = json!({ "action_type": 2, "filelist": [entry.id], "exclude_fids": [] });
  tasks::settle(api, &api.post("/file/delete", &body).await?).await?;
  paths::forget(api);
  Ok(
    Action::done("delete", entry.label())
      .with_id(entry.id)
      .with_message("moved to the recycle bin: `trash` lists it, `restore` brings it back"),
  )
}

/// A new folder at `path`; with `parents`, the folders above it too.
pub async fn mkdir(api: &Api, path: &str, parents: bool) -> Result<Data> {
  let path = refs::path(path);
  if path == "/" {
    return Err(Error::input("the root folder exists already"));
  }
  let (parent, name) = refs::split(&path);
  let body = if parents {
    json!({ "pdir_fid": "0", "file_name": "", "dir_path": path, "dir_init_lock": false })
  } else {
    let parent = paths::folder_at(api, &parent).await?;
    json!({ "pdir_fid": parent.id, "file_name": name, "dir_path": "", "dir_init_lock": false })
  };
  let v = api.post("/file", &body).await?;
  let mut action = Action::done("mkdir", &path);
  if let Some(fid) = v.str("data.fid") {
    action = action.with_id(fid);
  }
  Ok(Data::Action(action))
}

pub async fn rename(api: &Api, arg: &str, name: &str) -> Result<Data> {
  let name = name.trim();
  if name.is_empty() || name.contains('/') {
    return Err(Error::input(
      "the new name must be a plain name, without `/`",
    ));
  }
  let entry = own(api, arg).await?;
  let body = json!({ "fid": entry.id, "file_name": name });
  api.post("/file/rename", &body).await?;
  paths::forget(api);
  Ok(Data::Action(
    Action::done("rename", entry.label()).with_message(format!("now called {name}")),
  ))
}

/// Move (or with `copy`, copy) the first paths into the last, a folder.
pub async fn transfer(api: &Api, args: &[String], copy: bool) -> Result<Data> {
  let Some((dest, sources)) = args.split_last() else {
    return Err(Error::input("expected the files to move and a folder"));
  };
  let dest = folder(api, dest).await?;
  let mut fids = Vec::new();
  for source in sources {
    fids.push(own(api, source).await?.id);
  }
  let (path, action, body) = if copy {
    let body =
      json!({ "action_type": 2, "to_pdir_fid": dest.id, "filelist": fids, "copy_source": 1 });
    ("/file/copy", "copy", body)
  } else {
    let body =
      json!({ "action_type": 1, "to_pdir_fid": dest.id, "filelist": fids, "exclude_fids": [] });
    ("/file/move", "move", body)
  };
  tasks::settle(api, &api.post(path, &body).await?).await?;
  paths::forget(api);
  Ok(Data::Action(
    Action::done(action, sources.join(", ")).with_message(format!("into {}", dest.label())),
  ))
}

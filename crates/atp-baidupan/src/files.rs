//! Files and folders: one entry, a folder's entries, search by name, and
//! the changes (folders, renames, moves, copies, deletes). Each goes over
//! the web drive when its login is there, else over the open platform;
//! both take the same paths and answer alike. Changes name paths directly,
//! without looking them up first: Baidu says when one is missing.

use atp_core::{
  Action, Data, Error, ErrorCode, FileEntry, Page, PageReq, Result, Value, ValueExt, json,
};

use crate::api::{Api, OPEN_HINT, Route, refusal};
use crate::refs::{self, FileRef};
use crate::{parse, sharepages, tasks};

/// Entries per web listing call; the page number stays the cursor, so the
/// size never changes between calls.
const WEB_PAGE: usize = 100;
/// Entries per open-platform call: large, to spare the app's call quota.
const OPEN_PAGE: usize = 1000;
/// Pages of one folder searched for a name.
const MAX_PAGES: usize = 50;
/// The open platform's search takes at most this many characters.
const MAX_KEY: usize = 30;

/// The order of a listing: `name`, `time` or `size`, descending or not.
#[derive(Debug, Clone, Copy)]
pub struct Sort<'a> {
  pub by: &'a str,
  pub desc: bool,
}

/// The drive's root folder.
pub fn root() -> FileEntry {
  FileEntry {
    id: "/".into(),
    path: Some("/".into()),
    dir: true,
    ..FileEntry::default()
  }
}

/// A path, a file id or a share link.
pub async fn file(api: &Api, arg: &str) -> Result<FileEntry> {
  match refs::file(arg)? {
    FileRef::Path(path) => at(api, &path).await,
    FileRef::Id(fs_id) => by_id(api, &fs_id).await,
    FileRef::Share(share) => sharepages::read(api, &share).await,
  }
}

/// The path an argument names in your drive (`/` for the root): a path,
/// or a file id looked up on the open platform.
pub async fn path_of(api: &Api, arg: &str) -> Result<String> {
  match refs::file(arg)? {
    FileRef::Path(path) => Ok(path),
    FileRef::Id(fs_id) => Ok(by_id(api, &fs_id).await?.path.unwrap_or_default()),
    FileRef::Share(_) => Err(Error::input(format!(
      "{arg} is someone's share; `save` it into your drive first"
    ))),
  }
}

/// A file or folder of your drive to change: anything but the root.
pub async fn changeable(api: &Api, arg: &str) -> Result<String> {
  let path = path_of(api, &api.ctx.post_ref(arg)?).await?;
  if path == "/" {
    return Err(Error::input("the root folder cannot be changed"));
  }
  Ok(path)
}

/// The entry at `path`, found in its folder's listing.
pub async fn at(api: &Api, path: &str) -> Result<FileEntry> {
  if path == "/" {
    return Ok(root());
  }
  let (parent, name) = refs::split(path);
  let mut cursor = None;
  for _ in 0..MAX_PAGES {
    let req = PageReq {
      cursor,
      size: OPEN_PAGE,
    };
    let page = list(api, &parent, None, &req).await?;
    if let Some(found) = page.items.into_iter().find(|e| e.name == name) {
      return Ok(found);
    }
    match page.next_cursor {
      Some(next) if page.has_more => cursor = Some(next),
      _ => break,
    }
  }
  Err(Error::not_found(format!("no {path} in your drive")))
}

/// A file id's entry; only the open platform looks ids up.
async fn by_id(api: &Api, fs_id: &str) -> Result<FileEntry> {
  if !api.open().logged_in() {
    return Err(
      Error::auth("file ids are looked up through Baidu's open platform, which needs your own app")
        .with_hint(format!("use the path instead, or {OPEN_HINT}")),
    );
  }
  let item = meta(api, fs_id, false).await?;
  parse::file(&item).ok_or_else(|| Error::not_found(format!("no file {fs_id} in your drive")))
}

/// One file's details from the open platform (`filemetas`), with its
/// download link when `dlink`.
pub async fn meta(api: &Api, fs_id: &str, dlink: bool) -> Result<Value> {
  let query = [
    ("method", "filemetas".to_owned()),
    ("fsids", format!("[{fs_id}]")),
    ("dlink", if dlink { "1" } else { "0" }.to_owned()),
  ];
  let v = api.open().get("/multimedia", &query).await?;
  let Some(item) = v.list("list").first() else {
    return Err(Error::not_found(format!("no file {fs_id} in your drive")));
  };
  match item.i64("errno").unwrap_or(0) {
    0 => Ok(item.clone()),
    code => Err(refusal(item, code, "filemetas", Route::Open)),
  }
}

/// One page of a folder's entries.
pub async fn list(
  api: &Api,
  dir: &str,
  sort: Option<Sort<'_>>,
  req: &PageReq,
) -> Result<Page<FileEntry>> {
  let (by, desc) = sort.map_or(("name", false), |s| (s.by, s.desc));
  let desc = if desc { "1" } else { "0" }.to_owned();
  let page = req.number_or(1);
  let (v, size) = match api.route()? {
    Route::Web => {
      let query = [
        ("dir", dir.to_owned()),
        ("order", by.to_owned()),
        ("desc", desc),
        ("num", WEB_PAGE.to_string()),
        ("page", page.to_string()),
        ("showempty", "0".into()),
      ];
      (api.get("/api/list", &query).await?, WEB_PAGE)
    }
    Route::Open => {
      let start = (page - 1) * OPEN_PAGE as u64;
      let query = [
        ("method", "list".to_owned()),
        ("dir", dir.to_owned()),
        ("order", by.to_owned()),
        ("desc", desc),
        ("start", start.to_string()),
        ("limit", OPEN_PAGE.to_string()),
        ("folder", "0".into()),
      ];
      (api.open().get("/file", &query).await?, OPEN_PAGE)
    }
  };
  let items: Vec<FileEntry> = v.list("list").iter().filter_map(parse::file).collect();
  Ok(numbered(items, page, size))
}

/// A page numbered from 1 of a listing serving `size` entries a call: a
/// full page may have more after it.
pub fn numbered<T>(items: Vec<T>, page: u64, size: usize) -> Page<T> {
  let more = items.len() >= size;
  Page::new(items, more.then(|| (page + 1).to_string()))
}

/// Files and folders of the whole drive whose names contain `keyword`.
pub async fn search(api: &Api, keyword: &str, req: &PageReq) -> Result<Page<FileEntry>> {
  let page = req.number_or(1);
  let mut query = vec![
    ("key", keyword.to_owned()),
    ("page", page.to_string()),
    ("recursion", "1".to_owned()),
  ];
  let (v, size) = match api.route()? {
    Route::Web => {
      query.extend([
        ("num", WEB_PAGE.to_string()),
        ("order", "time".into()),
        ("desc", "1".into()),
      ]);
      (api.get("/api/search", &query).await?, WEB_PAGE)
    }
    Route::Open => {
      if keyword.chars().count() > MAX_KEY {
        return Err(Error::input(format!(
          "the open platform searches for at most {MAX_KEY} characters"
        )));
      }
      query.extend([("method", "search".into()), ("dir", "/".into())]);
      let v = api
        .open()
        .get("/file", &query)
        .await
        .map_err(search_limit)?;
      (v, 500)
    }
  };
  let items: Vec<FileEntry> = v.list("list").iter().filter_map(parse::file).collect();
  match v.bool("has_more") {
    Some(more) => Ok(Page::new(items, more.then(|| (page + 1).to_string()))),
    None => Ok(numbered(items, page, size)),
  }
}

fn search_limit(e: Error) -> Error {
  match e.code {
    ErrorCode::RateLimited => e.with_hint("the open platform allows 2000 searches a day per user"),
    _ => e,
  }
}

/// Move a file or folder to the recycle bin.
pub async fn delete(api: &Api, arg: &str) -> Result<Action> {
  let path = changeable(api, arg).await?;
  manage(api, "delete", &json!([path])).await?;
  Ok(
    Action::done("delete", &path)
      .with_id(&path)
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
  refs::name(&name)?;
  if parents {
    for above in ancestors(&parent) {
      match create_dir(api, &above).await {
        Err(e) if exists(&e) => {}
        result => {
          result?;
        }
      }
    }
  } else if parent != "/" && !at(api, &parent).await?.dir {
    return Err(Error::input(format!("{parent} is a file, not a folder")));
  }
  let v = create_dir(api, &path).await?;
  let mut action = Action::done("mkdir", &path);
  if let Some(fs_id) = v.str("fs_id") {
    action = action.with_id(fs_id);
  }
  Ok(Data::Action(action))
}

/// `/a/b` → `/a`, `/a/b`.
fn ancestors(path: &str) -> Vec<String> {
  let names: Vec<&str> = path.split('/').filter(|n| !n.is_empty()).collect();
  (1..=names.len())
    .map(|depth| format!("/{}", names[..depth].join("/")))
    .collect()
}

/// Whether Baidu refused a name because it is taken.
fn exists(e: &Error) -> bool {
  e.message.ends_with("(errno -8)") || e.message.ends_with("(errno 31061)")
}

async fn create_dir(api: &Api, path: &str) -> Result<Value> {
  let form = [
    ("path", path.to_owned()),
    ("isdir", "1".to_owned()),
    ("rtype", "0".to_owned()),
  ];
  match api.route()? {
    Route::Web => {
      let query = [
        ("a", "commit".to_owned()),
        ("bdstoken", api.bdstoken().await?),
      ];
      let mut form = form.to_vec();
      form.push(("block_list", "[]".into()));
      api.post("/api/create", &query, &form).await
    }
    Route::Open => {
      let query = [("method", "create".to_owned())];
      api.open().post("/file", &query, &form).await
    }
  }
}

pub async fn rename(api: &Api, arg: &str, name: &str) -> Result<Data> {
  let name = refs::name(name)?;
  let path = changeable(api, arg).await?;
  manage(api, "rename", &json!([{ "path": path, "newname": name }])).await?;
  Ok(Data::Action(
    Action::done("rename", &path).with_message(format!("now called {name}")),
  ))
}

/// Move (or with `copy`, copy) the first paths into the last, a folder.
pub async fn transfer(api: &Api, args: &[String], copy: bool) -> Result<Data> {
  let Some((dest, sources)) = args.split_last() else {
    return Err(Error::input("expected the files to move and a folder"));
  };
  let dest = path_of(api, &api.ctx.post_ref(dest)?).await?;
  let mut list = Vec::new();
  for source in sources {
    let path = changeable(api, source).await?;
    let name = refs::split(&path).1;
    list.push(json!({ "path": path, "dest": dest, "newname": name }));
  }
  let opera = if copy { "copy" } else { "move" };
  manage(api, opera, &Value::Array(list)).await?;
  Ok(Data::Action(
    Action::done(opera, sources.join(", ")).with_message(format!("into {dest}")),
  ))
}

/// The file manager: `opera` copy, move, rename or delete on `list`. The
/// web drive runs it as a background task that is waited for; the open
/// platform is asked to finish before it answers.
pub async fn manage(api: &Api, opera: &str, list: &Value) -> Result<()> {
  let mut form = vec![("filelist", list.to_string())];
  if opera != "delete" {
    form.push(("ondup", "fail".into()));
  }
  match api.route()? {
    Route::Web => {
      let query = [
        ("opera", opera.to_owned()),
        ("async", "2".into()),
        ("onnest", "fail".into()),
        ("newVerify", "1".into()),
        ("bdstoken", api.bdstoken().await?),
      ];
      let v = api.post("/api/filemanager", &query, &form).await?;
      tasks::settle(api, &v).await?;
    }
    Route::Open => {
      let query = [
        ("method", "filemanager".to_owned()),
        ("opera", opera.into()),
      ];
      form.push(("async", "0".into()));
      let v = api.open().post("/file", &query, &form).await?;
      tasks::items_done(&v, opera, Route::Open)?;
    }
  }
  Ok(())
}

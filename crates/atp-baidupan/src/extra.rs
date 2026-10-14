//! Baidu Netdisk-only commands: folders, moves, uploads, shares, the
//! recycle bin, offline downloads.

use std::path::PathBuf;

use atp_core::cli::PageArgs;
use atp_core::{Data, Result};

use crate::api::Api;
use crate::files::{self, Sort};
use crate::{offline, refs, sharepages, shares, trash, uploads, users};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// List a folder (the root without PATH)
  Ls {
    /// A path (`/Movies`), a file id or `#N`
    path: Option<String>,
    /// Order by name, time or size (folders first)
    #[arg(long, value_parser = ["name", "time", "size"])]
    sort: Option<String>,
    /// Descending order
    #[arg(long)]
    desc: bool,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Create a folder
  #[command(after_long_help = EX_MKDIR)]
  Mkdir {
    /// Path of the new folder (`/Movies/2024`)
    path: String,
    /// Create the folders above it too
    #[arg(short, long)]
    parents: bool,
  },
  /// Move files and folders into a folder: `mv SRC… DEST`
  #[command(after_long_help = EX_MV)]
  Mv {
    /// Paths, file ids or #N to move, then the folder to move them into
    #[arg(value_name = "PATH", num_args = 2.., required = true)]
    paths: Vec<String>,
  },
  /// Copy files and folders into a folder: `cp SRC… DEST`
  Cp {
    /// Paths, file ids or #N to copy, then the folder to copy them into
    #[arg(value_name = "PATH", num_args = 2.., required = true)]
    paths: Vec<String>,
  },
  /// Rename a file or folder
  Rename {
    /// A path, file id or #N
    path: String,
    /// The new name (not a path)
    name: String,
  },
  /// Upload local files through the open platform (at once when the drive holds them already)
  #[command(after_long_help = EX_UPLOAD)]
  Upload {
    /// Local files to upload (up to 4 GB each)
    #[arg(required = true)]
    files: Vec<PathBuf>,
    /// Folder to upload into (default: the root)
    #[arg(long, value_name = "DIR")]
    to: Option<String>,
    /// Replace files of the same name instead of renaming the new one
    #[arg(long)]
    overwrite: bool,
  },
  /// Share files and folders in one link with an extraction code
  #[command(after_long_help = EX_SHARE)]
  Share {
    /// Paths, file ids or #N to share
    #[arg(value_name = "PATH", required = true)]
    paths: Vec<String>,
    /// Days the link works: 0 (no end), 1, 7 or 30
    #[arg(long, value_name = "DAYS", default_value_t = 7)]
    expire: u32,
    /// The extraction code (4 letters or digits; a random one without it)
    #[arg(long, value_name = "CODE")]
    pwd: Option<String>,
  },
  /// Your shares
  Shares {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Cancel shares by their ids from `shares`
  Unshare {
    /// Share ids or #N from `shares`
    #[arg(required = true)]
    ids: Vec<String>,
    /// Skip the confirmation question (needed in scripts)
    #[arg(short, long)]
    yes: bool,
  },
  /// Save everything in someone's share into your drive (转存)
  #[command(after_long_help = EX_SAVE)]
  Save {
    /// Share link (`https://pan.baidu.com/s/1…`)
    url: String,
    /// The share's extraction code
    #[arg(long, value_name = "CODE")]
    pwd: Option<String>,
    /// Folder to save into (default: the root)
    #[arg(long, value_name = "DIR")]
    to: Option<String>,
  },
  /// List what someone's share holds
  #[command(name = "share-ls", after_long_help = EX_SHARE_LS)]
  ShareLs {
    /// Share link (`https://pan.baidu.com/s/1…`, `?pwd=` or `提取码` included or not)
    url: String,
    /// A folder inside the share: its path from an earlier `share-ls`, or `#N`
    dir: Option<String>,
    /// The share's extraction code
    #[arg(long, value_name = "CODE")]
    pwd: Option<String>,
    #[command(flatten)]
    page: PageArgs,
  },
  /// The recycle bin
  Trash {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Restore entries of the recycle bin by their ids from `trash`
  Restore {
    /// File ids or #N from `trash`
    #[arg(required = true)]
    ids: Vec<String>,
  },
  /// Delete entries of the recycle bin for good
  Purge {
    /// File ids or #N from `trash`
    #[arg(required = true)]
    ids: Vec<String>,
    /// Skip the confirmation question (needed in scripts)
    #[arg(short, long)]
    yes: bool,
  },
  /// Storage used and left
  Quota,
  /// Offline downloads: Baidu fetches links (http, magnet, ed2k …) into your drive
  #[command(subcommand, after_long_help = EX_OFFLINE)]
  Offline(Offline),
}

#[derive(Debug, clap::Subcommand)]
pub enum Offline {
  /// Hand links to Baidu to fetch into your drive
  #[command(after_long_help = EX_OFFLINE_ADD)]
  Add {
    /// Links to fetch: http(s), ftp, magnet or ed2k
    #[arg(value_name = "URL", required = true)]
    urls: Vec<String>,
    /// Folder to save into (default: the root)
    #[arg(long, value_name = "DIR")]
    to: Option<String>,
  },
  /// The offline downloads and how far they got
  List {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Cancel offline downloads by their ids from `offline list`
  Cancel {
    /// Task ids or #N from `offline list`
    #[arg(required = true)]
    ids: Vec<String>,
    /// Skip the confirmation question (needed in scripts)
    #[arg(short, long)]
    yes: bool,
  },
}

const EX_MKDIR: &str = "\
Examples:
  atp baidupan mkdir /Movies
  atp baidupan mkdir /Movies/2024/Summer -p

More: atp guide baidupan mkdir";

const EX_MV: &str = "\
Examples:
  atp baidupan mv /Downloads/a.mkv /Movies
  atp baidupan mv '#1' '#2' /Archive

More: atp guide baidupan mv";

const EX_UPLOAD: &str = "\
Examples:
  atp baidupan upload movie.mkv
  atp baidupan upload *.jpg --to /Photos --overwrite

More: atp guide baidupan upload";

const EX_SHARE: &str = "\
Examples:
  atp baidupan share /Movies/a.mkv
  atp baidupan share /Photos /Docs --expire 0 --pwd ab12

More: atp guide baidupan share";

const EX_SAVE: &str = "\
Examples:
  atp baidupan save 'https://pan.baidu.com/s/1AbCdEf?pwd=ab12'
  atp baidupan save https://pan.baidu.com/s/1AbCdEf --pwd ab12 --to /Saved

More: atp guide baidupan save";

const EX_SHARE_LS: &str = "\
Examples:
  atp baidupan share-ls https://pan.baidu.com/s/1AbCdEf --pwd ab12
  atp baidupan share-ls https://pan.baidu.com/s/1AbCdEf '#2' --pwd ab12

More: atp guide baidupan share-ls";

const EX_OFFLINE: &str = "\
Examples:
  atp baidupan offline add 'magnet:?xt=urn:btih:...' --to /Downloads
  atp baidupan offline list
  atp baidupan offline cancel '#1'

More: atp guide baidupan offline";

const EX_OFFLINE_ADD: &str = "\
Examples:
  atp baidupan offline add https://example.com/file.iso
  atp baidupan offline add 'magnet:?xt=urn:btih:...' --to /Downloads

More: atp guide baidupan offline add";

pub async fn run(api: &Api, command: Command) -> Result<Data> {
  Ok(match command {
    Command::Ls {
      path,
      sort,
      desc,
      page,
    } => ls(api, path, sort.as_deref(), desc, page).await?,
    Command::Mkdir { path, parents } => files::mkdir(api, &path, parents).await?,
    Command::Mv { paths } => files::transfer(api, &paths, false).await?,
    Command::Cp { paths } => files::transfer(api, &paths, true).await?,
    Command::Rename { path, name } => files::rename(api, &path, &name).await?,
    Command::Upload {
      files,
      to,
      overwrite,
    } => uploads::upload(api, &files, to.as_deref(), overwrite).await?,
    Command::Share { paths, expire, pwd } => {
      Data::File(Box::new(shares::share(api, &paths, expire, pwd).await?))
    }
    Command::Shares { page } => {
      Data::Files(page.collect(async |r| shares::list(api, &r).await).await?)
    }
    Command::Unshare { ids, yes } => Data::Action(shares::unshare(api, &ids, yes).await?),
    Command::Save { url, pwd, to } => {
      let share = refs::share(&url, pwd)?;
      Data::Action(sharepages::save(api, &share, to.as_deref()).await?)
    }
    Command::ShareLs {
      url,
      dir,
      pwd,
      page,
    } => share_ls(api, &url, dir, pwd, page).await?,
    Command::Trash { page } => {
      Data::Files(page.collect(async |r| trash::list(api, &r).await).await?)
    }
    Command::Restore { ids } => Data::Action(trash::restore(api, &ids).await?),
    Command::Purge { ids, yes } => Data::Action(trash::purge(api, &ids, yes).await?),
    Command::Quota => users::quota(api).await?,
    Command::Offline(command) => run_offline(api, command).await?,
  })
}

async fn run_offline(api: &Api, command: Offline) -> Result<Data> {
  Ok(match command {
    Offline::Add { urls, to } => Data::Action(offline::add(api, &urls, to.as_deref()).await?),
    Offline::List { page } => {
      Data::Files(page.collect(async |r| offline::list(api, &r).await).await?)
    }
    Offline::Cancel { ids, yes } => Data::Action(offline::cancel(api, &ids, yes).await?),
  })
}

async fn ls(
  api: &Api,
  path: Option<String>,
  sort: Option<&str>,
  desc: bool,
  page: PageArgs,
) -> Result<Data> {
  let dir = match path {
    Some(arg) => files::path_of(api, &api.ctx.post_ref(&arg)?).await?,
    None => "/".to_owned(),
  };
  let sort = sort
    .map(|by| Sort { by, desc })
    .or(desc.then_some(Sort { by: "name", desc }));
  let listing = page
    .collect(async |r| files::list(api, &dir, sort, &r).await)
    .await?;
  Ok(Data::Files(listing))
}

async fn share_ls(
  api: &Api,
  url: &str,
  dir: Option<String>,
  pwd: Option<String>,
  page: PageArgs,
) -> Result<Data> {
  let share = refs::share(url, pwd)?;
  let dir = dir.map(|d| api.ctx.post_ref(&d)).transpose()?;
  let opened = sharepages::open(api, &share, dir.as_deref()).await?;
  let listing = page
    .collect(async |r| sharepages::page(api, &opened, &r).await)
    .await?;
  Ok(Data::Files(listing))
}

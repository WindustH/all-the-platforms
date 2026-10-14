//! Quark-only commands: folders, moves, uploads, shares, the recycle bin.

use std::path::PathBuf;

use atp_core::cli::PageArgs;
use atp_core::{Data, Result};

use crate::api::Api;
use crate::files::{self, Sort};
use crate::{paths, refs, sharepages, shares, trash, uploads, users};

#[derive(Debug, clap::Subcommand)]
pub enum Command {
  /// List a folder (the root without PATH)
  Ls {
    /// A path (`/Movies`), a folder id or `#N`
    path: Option<String>,
    /// Order by name or by time (folders always first)
    #[arg(long, value_parser = ["name", "time"])]
    sort: Option<String>,
    /// Descending order
    #[arg(long)]
    desc: bool,
    #[command(flatten)]
    page: PageArgs,
  },
  /// Create a folder
  Mkdir {
    path: String,
    /// Create the folders above it too
    #[arg(short, long)]
    parents: bool,
  },
  /// Move files and folders into a folder: `mv SRC… DEST`
  Mv {
    #[arg(value_name = "PATH", num_args = 2.., required = true)]
    paths: Vec<String>,
  },
  /// Copy files and folders into a folder: `cp SRC… DEST`
  Cp {
    #[arg(value_name = "PATH", num_args = 2.., required = true)]
    paths: Vec<String>,
  },
  /// Rename a file or folder
  Rename { path: String, name: String },
  /// Upload local files (at once when the drive holds them already)
  Upload {
    #[arg(required = true)]
    files: Vec<PathBuf>,
    /// Folder to upload into (default: the root)
    #[arg(long, value_name = "DIR")]
    to: Option<String>,
  },
  /// Share files and folders in one link
  Share {
    #[arg(value_name = "PATH", required = true)]
    paths: Vec<String>,
    /// Days the link works: 0 (no end), 1, 7 or 30
    #[arg(long, value_name = "DAYS", default_value_t = 7)]
    expire: u32,
    /// Ask for an extraction code: this one, or a random one without a value
    #[arg(long, value_name = "CODE", num_args = 0..=1, default_missing_value = "")]
    pwd: Option<String>,
  },
  /// Your shares
  Shares {
    #[command(flatten)]
    page: PageArgs,
  },
  /// Cancel shares by their ids from `shares`
  Unshare {
    #[arg(required = true)]
    ids: Vec<String>,
    #[arg(short, long)]
    yes: bool,
  },
  /// Save everything in someone's share into your drive (转存)
  Save {
    /// Share link (`https://pan.quark.cn/s/…`)
    url: String,
    /// The share's extraction code
    #[arg(long, value_name = "CODE")]
    pwd: Option<String>,
    /// Folder to save into (default: the root)
    #[arg(long, value_name = "DIR")]
    to: Option<String>,
  },
  /// List what someone's share holds
  #[command(name = "share-ls")]
  ShareLs {
    url: String,
    /// A folder inside the share: its id from an earlier `share-ls`, or `#N`
    dir: Option<String>,
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
    #[arg(required = true)]
    ids: Vec<String>,
  },
  /// Delete entries of the recycle bin for good
  Purge {
    #[arg(required = true)]
    ids: Vec<String>,
    #[arg(short, long)]
    yes: bool,
  },
  /// Storage used and left, and the member type
  Quota,
}

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
    Command::Upload { files, to } => uploads::upload(api, &files, to.as_deref()).await?,
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
  })
}

async fn ls(
  api: &Api,
  path: Option<String>,
  sort: Option<&str>,
  desc: bool,
  page: PageArgs,
) -> Result<Data> {
  let folder = match path {
    Some(arg) => files::folder(api, &arg).await?,
    None => paths::root(),
  };
  let sort = sort.map(|by| Sort { by, desc });
  let listing = page
    .collect(async |r| files::list(api, &folder, sort, &r).await)
    .await?;
  paths::remember(api, &listing.items);
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
  let listing = page
    .collect(async |r| sharepages::list(api, &share, dir.as_deref(), &r).await)
    .await?;
  Ok(Data::Files(listing))
}

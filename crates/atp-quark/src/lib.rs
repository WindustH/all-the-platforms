//! Quark Drive (夸克网盘) for all-the-platforms: the drive of a logged-in
//! account through the web drive's own calls (`drive-pc.quark.cn`, cookie
//! `__pus`), no signatures.
//!
//! Files and folders are [`FileEntry`]s known by their id (`fid`); paths
//! are found by listing folder after folder and cached ([`paths`]). Moves,
//! copies, deletes, shares and saves of shared files are background tasks
//! the commands wait for ([`tasks`]). Downloads ask for links with the PC
//! client's User-Agent (the web one is refused for big files) and fetch
//! them one range at a time; uploads go to Aliyun OSS in parts after the
//! drive was asked whether it holds the file already.

mod api;
mod downloads;
mod extra;
mod files;
mod login;
mod oss;
mod parse;
mod paths;
mod refs;
mod sharepages;
mod shares;
mod tasks;
mod trash;
mod uploads;
mod users;

/// The commands only this platform has, for [`Platform::run_extra`](atp_core::Platform::run_extra).
pub use extra::Command;

use std::time::Duration;

use atp_core::download::Link;
use atp_core::{
  Action, Cap, Choices, Ctx, Data, FileEntry, Page, PageReq, Platform, PlatformInfo, PlatformKind,
  QrStatus, QrTicket, Query, Result, User,
};

use crate::api::Api;

pub struct Quark {
  api: Api,
}

impl Platform for Quark {
  const INFO: PlatformInfo = PlatformInfo {
    id: "quark",
    name: "Quark Drive",
    aliases: &["kuake"],
    about: "Quark Drive (夸克网盘): files and folders, uploads, downloads, shares, saving shared files",
    home: "https://pan.quark.cn",
    cookie_domains: &["quark.cn"],
    required_cookies: &["__pus"],
    caps: &[
      Cap::QrLogin,
      Cap::Search,
      Cap::Read,
      Cap::Download,
      Cap::Delete,
      Cap::Files,
    ],
    choices: Choices::NONE,
    min_interval: Duration::from_millis(500),
    kind: PlatformKind::Drive,
    token: "",
    env: &[],
    guest: "nothing",
    guide: include_str!("../GUIDE.md"),
  };

  type Extra = Command;

  fn new(ctx: Ctx) -> Result<Self> {
    Ok(Self { api: Api::new(ctx) })
  }

  fn ctx(&self) -> &Ctx {
    &self.api.ctx
  }

  async fn whoami(&self) -> Result<User> {
    users::whoami(&self.api).await
  }

  async fn qr_start(&self) -> Result<QrTicket> {
    login::qr_start(&self.api).await
  }

  async fn qr_poll(&self, ticket: &QrTicket) -> Result<QrStatus> {
    login::qr_poll(&self.api, ticket).await
  }

  async fn delete(&self, post: &str) -> Result<Action> {
    files::delete(&self.api, post).await
  }

  async fn file(&self, file: &str) -> Result<FileEntry> {
    files::file(&self.api, file).await
  }

  async fn files(&self, folder: &FileEntry, page: &PageReq) -> Result<Page<FileEntry>> {
    downloads::children(&self.api, folder, page).await
  }

  async fn search_files(&self, query: &Query, page: &PageReq) -> Result<Page<FileEntry>> {
    files::search(&self.api, &query.keyword, page).await
  }

  async fn file_link(&self, file: &FileEntry, renew: bool) -> Result<Link> {
    downloads::link(&self.api, file, renew).await
  }

  async fn run_extra(&self, command: Self::Extra) -> Result<Data> {
    extra::run(&self.api, command).await
  }
}

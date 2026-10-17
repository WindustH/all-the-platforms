//! Baidu Netdisk (百度网盘) for all-the-platforms, over two routes that one
//! session holds side by side:
//!
//! - the web drive's own calls (`pan.baidu.com/api/*`, cookies `BDUSS` and
//!   `STOKEN`, [`api`]): files and folders, the recycle bin, shares, saving
//!   shared files, offline downloads;
//! - Baidu's open platform (`pan.baidu.com/rest/2.0/xpan/*`, an OAuth token
//!   of the user's own app, [`open`]): downloads and uploads, and the file
//!   commands when there is no web login.
//!
//! Each command takes the route that can do it ([`api::Route`]) and names
//! the login it misses. Files are known by their path; listings carry the
//! drive's file id (`fs_id`) for downloads. Downloads use the documented
//! `dlink` with `User-Agent: pan.baidu.com`, one connection, sequential
//! ranges: nothing here changes the account's download speed.

mod api;
mod downloads;
mod extra;
mod files;
mod login;
mod offline;
mod open;
mod parse;
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

pub struct Baidupan {
  api: Api,
}

impl Platform for Baidupan {
  const INFO: PlatformInfo = PlatformInfo {
    id: "baidupan",
    name: "Baidu Netdisk",
    aliases: &["bdpan", "baiduyun"],
    about: "Baidu Netdisk (百度网盘): files and folders, downloads and uploads through your own open-platform app, shares, saving shared files, offline downloads",
    home: "https://pan.baidu.com",
    // The drive's own cookies win over passport's of the same name (`STOKEN`).
    cookie_domains: &["pan.baidu.com", "baidu.com"],
    required_cookies: &["BDUSS"],
    caps: &[
      Cap::QrLogin,
      Cap::TokenLogin,
      Cap::Search,
      Cap::Read,
      Cap::Download,
      Cap::Delete,
      Cap::Files,
    ],
    choices: Choices::NONE,
    min_interval: Duration::from_millis(500),
    kind: PlatformKind::Drive,
    token: "APP_KEY:SECRET_KEY",
    env: &["BAIDUPAN_APP_KEY", "BAIDUPAN_SECRET_KEY"],
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

  fn logged_in(&self) -> bool {
    self.api.web_login() || self.api.open().logged_in()
  }

  async fn whoami(&self) -> Result<User> {
    users::whoami(&self.api).await
  }

  async fn prepare_login(&self) -> Result<()> {
    login::prepare(&self.api).await
  }

  async fn login_token(&self, token: &str) -> Result<()> {
    login::token(&self.api, token).await
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

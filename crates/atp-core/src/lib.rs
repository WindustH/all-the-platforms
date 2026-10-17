//! Platform-agnostic kernel of all-the-platforms.
//!
//! - [`platform`]: the [`Platform`] trait a platform crate implements, plus its static [`PlatformInfo`].
//! - [`model`]: normalized posts, users, comments ... printed the same way everywhere.
//! - [`http`]: Chrome-fingerprinted HTTP with cookie jar, pacing and retries.
//! - [`json`] / [`text`]: helpers for picking apart upstream payloads.
//! - [`app`] / [`cli`]: the shared command set and dispatch; [`output`]: tables and the JSON/YAML envelope.

mod account;
pub mod app;
pub mod browser;
pub mod bulk;
pub mod chat;
pub mod cli;
pub mod credentials;
pub mod ctx;
pub mod download;
pub mod error;
pub mod feed;
pub mod file;
pub mod geo;
pub mod guide;
pub mod http;
pub mod json;
pub mod live;
mod mcp;
pub mod model;
pub mod music;
pub mod oauth;
pub mod open;
pub mod output;
pub mod paging;
pub mod platform;
mod platforms;
pub mod proto;
pub mod qr;
pub mod socket;
pub mod store;
pub mod task;
pub mod text;
pub mod token;
pub mod upload;
mod verify;

pub use app::App;
pub use ctx::Ctx;
pub use error::{Error, ErrorCode, Result};
pub use http::Http;
pub use json::ValueExt;
pub use model::*;
pub use open::{Open, open, save_session};
pub use platform::{
  Cap, Choices, Draft, Edit, FolderSpec, FollowQuery, Message, NoExtra, PageReq, Platform,
  PlatformInfo, PlatformKind, QrStatus, QrTicket, Query, Reply,
};
pub use serde_json::{Value, json};

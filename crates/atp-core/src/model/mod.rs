//! Normalized data shared by every platform.
//!
//! Platforms map their upstream payloads into these types, so every command
//! prints the same shape everywhere. Anything that does not fit a common field
//! goes into `extra`; the untouched upstream payload is kept in `raw` and only
//! printed with `--raw`.

mod action;
mod collection;
mod comment;
mod data;
mod file;
mod insights;
mod notification;
mod page;
mod post;
mod user;

use serde_json::Value;
use std::collections::BTreeMap;

pub use self::action::*;
pub use self::collection::*;
pub use self::comment::*;
pub use self::data::*;
pub use self::file::*;
pub use self::insights::*;
pub use self::notification::*;
pub use self::page::*;
pub use self::post::*;
pub use self::user::*;

pub type Extra = BTreeMap<String, Value>;

fn is_false(b: &bool) -> bool {
  !*b
}

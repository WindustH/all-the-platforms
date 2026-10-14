//! Everything a command can print.

use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

use super::*;

/// A timed line of a transcript / subtitle track.
#[derive(Debug, Clone, Serialize)]
pub struct Cue {
  pub from: f64,
  pub to: f64,
  pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Transcript {
  pub lang: String,
  pub cues: Vec<Cue>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Downloaded {
  pub kind: MediaKind,
  pub path: String,
  pub bytes: u64,
}

/// Everything a command can print. Serialized as the envelope's `data`.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum Data {
  Posts(Page<Post>),
  Users(Page<User>),
  Comments(Page<Comment>),
  Collections(Page<Collection>),
  Notifications(Page<Notification>),
  /// Files and folders of a drive, or its shares.
  Files(Page<FileEntry>),
  Post(Box<Post>),
  User(Box<User>),
  File(Box<FileEntry>),
  Action(Action),
  Auth(AuthStatus),
  Counts(BTreeMap<String, u64>),
  Transcript(Transcript),
  Downloads(Vec<Downloaded>),
  Insights(Box<Insights>),
  /// Routes between two places (map platforms).
  Routes(Box<crate::geo::Routes>),
  /// Distances between places (map platforms).
  Distances(Vec<crate::geo::Distance>),
  /// Anything else; printed as YAML in the terminal.
  Value(Value),
}

impl Data {
  /// Where a listing continues (`--cursor`), when it stopped before its end.
  pub fn next_cursor(&self) -> Option<&str> {
    match self {
      Data::Posts(p) => p.next_cursor.as_deref(),
      Data::Users(p) => p.next_cursor.as_deref(),
      Data::Comments(p) => p.next_cursor.as_deref(),
      Data::Collections(p) => p.next_cursor.as_deref(),
      Data::Notifications(p) => p.next_cursor.as_deref(),
      Data::Files(p) => p.next_cursor.as_deref(),
      _ => None,
    }
  }

  /// Drop upstream payloads (kept only with `--raw`).
  pub fn strip_raw(&mut self) {
    fn user(u: &mut User) {
      u.raw = None;
    }
    fn post(p: &mut Post) {
      p.raw = None;
      if let Some(a) = &mut p.author {
        user(a);
      }
      if let Some(q) = &mut p.quoted {
        post(q);
      }
    }
    fn comment(c: &mut Comment) {
      c.raw = None;
      if let Some(a) = &mut c.author {
        user(a);
      }
      c.replies.iter_mut().for_each(comment);
    }
    match self {
      Data::Posts(p) => p.items.iter_mut().for_each(post),
      Data::Users(p) => p.items.iter_mut().for_each(user),
      Data::Comments(p) => p.items.iter_mut().for_each(comment),
      Data::Collections(p) => p.items.iter_mut().for_each(|c| {
        c.raw = None;
        if let Some(o) = &mut c.owner {
          user(o);
        }
      }),
      Data::Notifications(p) => p.items.iter_mut().for_each(|n| {
        n.raw = None;
        if let Some(a) = &mut n.actor {
          user(a);
        }
      }),
      Data::Files(p) => p.items.iter_mut().for_each(|f| f.raw = None),
      Data::File(f) => f.raw = None,
      Data::Post(p) => post(p),
      Data::User(u) => user(u),
      Data::Auth(a) => {
        if let Some(u) = &mut a.user {
          user(u);
        }
      }
      Data::Insights(i) => i.raw = None,
      _ => {}
    }
  }
}

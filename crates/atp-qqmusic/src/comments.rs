//! Comments of songs, albums and playlists: hot, newest and recommended
//! pages, the replies under a comment, commenting and deleting.

use atp_core::{Action, Comment, Error, Page, PageReq, Result, Value, ValueExt, json};

use crate::api::{Api, Call};
use crate::refs::{Key, PostRef};
use crate::{albums, songs};

/// `comments --sort` values; the first is the default.
pub const SORTS: &[&str] = &["hot", "new", "recommended"];
/// Comments one page fetches at most.
const PAGE_SIZE: usize = 50;
/// Business types of comment threads.
const SONG: u64 = 1;
const ALBUM: u64 = 2;
const PLAYLIST: u64 = 3;
/// `RankType` of replies in time order.
const BY_TIME: u64 = 0;

/// A comment thread: what it belongs to (`BizType`) and its number.
pub struct Thread {
  kind: u64,
  id: String,
}

impl Thread {
  pub fn playlist(id: u64) -> Self {
    Thread {
      kind: PLAYLIST,
      id: id.to_string(),
    }
  }
}

/// The thread of a song, an album or a playlist; songs and albums by mid
/// are looked up for their numbers.
pub async fn thread(api: &Api, post: &PostRef) -> Result<Thread> {
  let (kind, id) = match post {
    PostRef::Song(Key::Id(id)) => (SONG, *id),
    PostRef::Song(key) => (SONG, songs::number(&songs::one(api, key).await?)?.0),
    PostRef::Album(Key::Id(id)) => (ALBUM, *id),
    PostRef::Album(key) => {
      let album = albums::detail(api, key).await?;
      let id = album.extra.get("album_id").and_then(|v| v.u64(""));
      (
        ALBUM,
        id.ok_or_else(|| Error::upstream("QQ Music sent an album without its number"))?,
      )
    }
    PostRef::Playlist(id) => (PLAYLIST, *id),
    PostRef::Singer(_) | PostRef::Toplist(_) => {
      return Err(Error::input(
        "comments belong to songs, albums and playlists",
      ));
    }
  };
  Ok(Thread {
    kind,
    id: id.to_string(),
  })
}

/// The call that counts a thread's comments (to send with others).
pub fn count_call<'a>(api: &'a Api, thread: &Thread) -> Call<'a> {
  let mut request = json!({"biz_id": thread.id, "biz_type": thread.kind});
  if thread.kind == SONG {
    request["biz_sub_type"] = json!(2);
  }
  api
    .call("music.globalComment.CommentCountSrv", "GetCmCount")
    .arg("request", request)
}

/// Where a page of comments starts: page number and size (kept from the
/// first page) and the sequence number and score of the last comment.
struct Cursor {
  page: u64,
  size: usize,
  seq: String,
  score: String,
}

impl Cursor {
  fn read(req: &PageReq) -> Cursor {
    let parts: Vec<&str> = req
      .cursor
      .as_deref()
      .unwrap_or_default()
      .split(':')
      .collect();
    Cursor {
      page: parts.first().and_then(|p| p.parse().ok()).unwrap_or(0),
      size: parts
        .get(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| req.size_within(PAGE_SIZE)),
      seq: parts.get(2).unwrap_or(&"").to_string(),
      score: parts.get(3).unwrap_or(&"").to_string(),
    }
  }

  /// The cursor after `list`, when there is more.
  fn after(&self, list: &[Value], more: bool) -> Option<String> {
    let last = list.last().filter(|_| more)?;
    Some(format!(
      "{}:{}:{}:{}",
      self.page + 1,
      self.size,
      last.str("SeqNo").unwrap_or_default(),
      last.str("RankScore").unwrap_or_default()
    ))
  }
}

/// One page of a thread.
pub async fn list(
  api: &Api,
  thread: &Thread,
  sort: Option<&str>,
  page: &PageReq,
) -> Result<Page<Comment>> {
  let at = Cursor::read(page);
  let read = "music.globalComment.CommentRead";
  let call = match sort.unwrap_or(SORTS[0]) {
    "new" => api
      .call(read, "GetNewCommentList")
      .arg("HashTagID", "")
      .arg("SelfSeeEnable", 1)
      .arg("AudioEnable", 1),
    "recommended" => api
      .call(read, "GetRecCommentList")
      .arg("Flag", 1)
      .arg("CmListUIVer", 1)
      .arg("AudioEnable", 1),
    _ => api
      .call(read, "GetHotCommentList")
      .arg("HotType", 1)
      .arg("WithAirborne", 0),
  };
  let data = call
    .arg("BizType", thread.kind)
    .arg("BizId", thread.id.as_str())
    .arg("PageSize", at.size)
    .arg("PageNum", at.page)
    .arg("LastCommentSeqNo", at.seq.as_str())
    .arg("PicEnable", 1)
    .send()
    .await?;
  Ok(page_of(&at, data.at("CommentList")))
}

fn page_of(at: &Cursor, list: &Value) -> Page<Comment> {
  let items = list.list("Comments");
  let more = list.bool("HasMore").unwrap_or(false);
  Page::new(
    items.iter().filter_map(crate::parse::comment).collect(),
    at.after(items, more),
  )
}

/// The replies under a comment, oldest first.
pub async fn replies(api: &Api, comment: &str, page: &PageReq) -> Result<Page<Comment>> {
  let at = Cursor::read(page);
  let data = api
    .call("music.globalComment.CommentRead", "GetReplyCommentList")
    .arg("RootCmId", comment)
    .arg("LastCommentSeqNo", at.seq.as_str())
    .arg("PageSize", at.size)
    .arg("LastRankScore", at.score.as_str())
    .arg("RankType", BY_TIME)
    .arg("PicEnable", 1)
    .arg("PageNum", at.page)
    .send()
    .await?;
  Ok(page_of(&at, data.at("CommentList")))
}

/// Comment on a thread, or reply to one of its comments.
pub async fn add(api: &Api, thread: &Thread, text: &str, reply_to: Option<&str>) -> Result<Action> {
  if text.trim().is_empty() {
    return Err(Error::input("the comment is empty"));
  }
  let mut call = api
    .call("music.globalComment.CommentWriteServer", "AddComment")
    .arg("BizType", thread.kind)
    .arg("BizId", thread.id.as_str())
    .arg("Content", text);
  if let Some(parent) = reply_to {
    call = call.arg("RepliedCmId", parent);
  }
  let data = call.write().send().await?;
  check(api, &data)?;
  let mut action = Action::done("comment", thread.id.as_str());
  if let Some(id) = data.first_str(&["CmId", "Comment.CmId", "CommentId"]) {
    action = action.with_id(id);
  }
  Ok(action)
}

/// Delete one of the account's comments (the shared command asked first).
pub async fn delete(api: &Api, comment: &str) -> Result<Action> {
  let data = api
    .call("music.globalComment.CommentWriteServer", "DelComment")
    .arg("CommentId", comment)
    .write()
    .send()
    .await?;
  check(api, &data)?;
  Ok(Action::done("delete-comment", comment))
}

/// A refusal inside a comment answer (`SubCode`).
fn check(api: &Api, data: &Value) -> Result<()> {
  match data.i64("SubCode").filter(|c| *c != 0) {
    Some(code) => Err(api.error(code, data)),
    None => Ok(()),
  }
}

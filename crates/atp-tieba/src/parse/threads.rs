//! Threads, their floors, and hot topics.

use super::*;

fn thread_media(thread: &Value, mut media: Vec<Media>) -> Vec<Media> {
  for item in thread.list("media") {
    if let Some(url) =
      item.first_str(&["origin_pic", "big_pic", "src_pic", "water_pic", "small_pic"])
      && !media.iter().any(|x| x.url == url)
    {
      let mut img = Media::image(url);
      img.width = item.u64("width").map(|w| w as u32).filter(|w| *w > 0);
      img.height = item.u64("height").map(|h| h as u32).filter(|h| *h > 0);
      media.push(img);
    }
  }
  let video = thread.at("video_info");
  if let Some(url) = video.str("video_url") {
    let mut clip = Media::video(url);
    clip.duration = video.f64("video_duration");
    clip.width = video.u64("video_width").map(|w| w as u32);
    clip.height = video.u64("video_height").map(|h| h as u32);
    clip.alt = video.str("thumbnail_url");
    media.push(clip);
  }
  media
}

/// A thread of a forum page, search, topic or user listing.
pub fn thread(thread: &Value, users: &Users) -> Post {
  let tid = thread
    .first_str(&["tid", "thread_id", "id"])
    .unwrap_or_default();
  let body = thread_body(thread);
  let mut extra = atp_core::Extra::new();
  let forum = thread
    .first_str(&["forum_name", "fname", "forum_info.forum_name", "forum.name"])
    .or_else(|| thread.str("forum_info.name"));
  if let Some(f) = forum {
    extra.insert("forum".into(), json!(f));
  }
  if let Some(fid) = thread.first_str(&["fid", "forum_id"]).filter(|f| f != "0") {
    extra.insert("forum_id".into(), json!(fid));
  }
  for key in ["is_top", "is_good"] {
    if thread.str(key).is_some_and(|s| s != "0") {
      extra.insert(key.trim_start_matches("is_").into(), json!(true));
    }
  }
  if let Some(pid) = thread.first_str(&["first_post_id", "post_id"]) {
    extra.insert("first_post_id".into(), json!(pid));
  }
  Post {
    kind: "thread".into(),
    title: thread.str("title"),
    text: Some(body.text).filter(|s| !s.is_empty()),
    url: Some(thread_url(&tid)),
    author: author(thread, users),
    created_at: secs(thread, &["create_time", "time"]),
    updated_at: secs(thread, &["last_time_int", "modified_time"]),
    metrics: Metrics {
      views: count(thread, &["view_num"]),
      likes: count(thread, &["agree_num", "agree.agree_num", "like_num"]),
      comments: count(thread, &["reply_num", "post_num"]),
      shares: count(thread, &["share_num"]),
      ..Metrics::default()
    },
    media: thread_media(thread, body.media),
    extra,
    raw: Some(thread.clone()),
    id: tid,
    ..Post::default()
  }
}

/// A floor (楼) of a thread, with the replies inlined under it.
pub fn floor(item: &Value, users: &Users, tid: &str) -> Comment {
  let pid = item.str("id").unwrap_or_default();
  let body = body(item.list("content"));
  let mut extra = atp_core::Extra::new();
  if let Some(n) = item.u64("floor") {
    extra.insert("floor".into(), json!(n));
  }
  extra.insert("url".into(), json!(floor_url(tid, &pid)));
  if !body.media.is_empty() {
    extra.insert(
      "images".into(),
      json!(body.media.iter().map(|m| m.url.clone()).collect::<Vec<_>>()),
    );
  }
  let author = author(item, users);
  Comment {
    location: author.as_ref().and_then(|a| a.location.clone()),
    author,
    text: body.text,
    created_at: secs(item, &["time"]),
    likes: count(item, &["agree.agree_num", "agree_num"]),
    reply_count: count(item, &["sub_post_number"]),
    reply_to: body.reply_to,
    replies: item
      .list("sub_post_list.sub_post_list")
      .iter()
      .map(|s| floor(s, users, tid))
      .collect(),
    extra,
    raw: Some(item.clone()),
    id: pid,
  }
}

/// A hot topic (热议) as a post; its threads come from `topic`.
pub fn topic(topic: &Value) -> Option<Post> {
  let id = topic.str("topic_id")?;
  let mut media = Vec::new();
  if let Some(pic) = topic.str("topic_pic") {
    media.push(Media::image(pic));
  }
  let mut extra = atp_core::Extra::new();
  if let Some(rank) = topic.u64("idx_num") {
    extra.insert("rank".into(), json!(rank));
  }
  Some(Post {
    kind: "topic".into(),
    title: topic.str("topic_name"),
    text: topic.first_str(&["topic_desc", "abstract"]),
    url: topic.str("topic_url").map(|u| u.replace("&amp;", "&")),
    created_at: secs(topic, &["create_time"]),
    metrics: Metrics {
      comments: count(topic, &["discuss_num"]),
      ..Metrics::default()
    },
    media,
    extra,
    raw: Some(topic.clone()),
    id,
    ..Post::default()
  })
}

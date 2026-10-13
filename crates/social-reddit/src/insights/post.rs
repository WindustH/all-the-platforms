//! One post: its public counters, and the post insights page for the author.

use jiff::Timestamp;
use jiff::tz::TimeZone;
use social_core::{ErrorCode, Insights, Post, Result, Value, ValueExt};

use super::{day, stats};
use crate::api::Api;
use crate::posts;

/// One post: its public counters, and the post insights page for the author.
pub async fn post(api: &Api, arg: &str) -> Result<Insights> {
  let post = posts::post(api, arg).await?;
  let d = post.raw.clone().unwrap_or_default();
  let mut ins = public(&post, &d);
  let author = post.author.as_ref().and_then(|a| a.handle.clone());
  let mine = match (api.logged_in(), author) {
    (true, Some(author)) => author.eq_ignore_ascii_case(&api.username().await?),
    _ => false,
  };
  let source = if mine {
    author_stats(api, &post.id, &mut ins).await?
  } else {
    "public counters: Reddit shows views, shares and traffic of a post to its author only".into()
  };
  ins.extra.insert("source".into(), source.into());
  ins.raw = Some(d);
  Ok(ins)
}

/// The counters anyone sees.
fn public(post: &Post, d: &Value) -> Insights {
  let tz = TimeZone::system();
  let mut ins = Insights {
    kind: "post".into(),
    subject: post.id.clone(),
    title: post.title.clone(),
    url: post.url.clone(),
    from: d
      .i64("created_utc")
      .and_then(|s| day(s, &tz))
      .map(|d| d.to_string()),
    to: Some(Timestamp::now().to_zoned(tz).date().to_string()),
    ..Insights::default()
  };
  ins.total("views", d.u64("view_count").map(Value::from));
  ins.total("likes", d.i64("score").map(Value::from));
  ins.total("upvote_ratio", d.f64("upvote_ratio").map(Value::from));
  ins.total("comments", d.u64("num_comments").map(Value::from));
  ins.total("crossposts", d.u64("num_crossposts").map(Value::from));
  ins.total("awards", d.u64("total_awards_received").map(Value::from));
  let x = &mut ins.extra;
  x.insert("likes".into(), "score (upvotes minus downvotes)".into());
  x.insert(
    "subreddit_subscribers".into(),
    d.u64("subreddit_subscribers").into(),
  );
  ins
}

/// Merge the author's post insights page; what the insights are made of.
async fn author_stats(api: &Api, id: &str, ins: &mut Insights) -> Result<String> {
  Ok(match stats::post_stats(api, id).await {
    Ok(Some(stats)) => {
      stats.merge_into(ins);
      "public counters and the author's post insights page".into()
    }
    Ok(None) => "public counters: Reddit has no insights for this post (it keeps them for about \
       90 days, and not for removed or deleted posts)"
      .into(),
    Err(e) if e.code == ErrorCode::NotAuthenticated => return Err(e),
    Err(e) => format!(
      "public counters: the post insights page failed: {}",
      e.message
    ),
  })
}

//! Analytics from what weibo.com exposes: the account's lifetime counters and
//! yesterday's reads and interactions (profile detail), and a post's
//! counters. Weibo's creator data center (me.weibo.com) has no web API that
//! works outside its own pages, so trends and audience data are missing.

use social_core::{Insights, Result, Value, ValueExt, json};

use crate::api::Client;
use crate::{people, posts};

const NO_DATA_CENTER: &str = "Weibo's creator data center (me.weibo.com) is not available here: \
   no daily trends or audience breakdowns, only the counters weibo.com shows";

pub async fn account(c: &Client) -> Result<Insights> {
  let uid = people::own_uid(c).await?;
  let info = c.get("/ajax/profile/info", &[("uid", &uid)]).await?;
  let user = info.at("data.user");
  let mut i = Insights {
    kind: "account".into(),
    subject: uid.clone(),
    title: user.str("screen_name"),
    url: Some(crate::refs::user_url(&uid)),
    totals_period: Some("lifetime".into()),
    warnings: vec![NO_DATA_CENTER.into()],
    ..Insights::default()
  };
  i.total("followers", user.count("followers_count").map(Value::from));
  i.total("following", user.count("friends_count").map(Value::from));
  i.total("posts", user.count("statuses_count").map(Value::from));
  for (metric, path) in [
    ("likes_received", "status_total_counter.like_cnt"),
    ("comments_received", "status_total_counter.comment_cnt"),
    ("reposts_received", "status_total_counter.repost_cnt"),
    ("interactions", "status_total_counter.total_cnt"),
  ] {
    i.total(metric, user.count(path).map(Value::from));
  }
  if let Ok(d) = c.get("/ajax/profile/detail", &[("uid", &uid)]).await {
    let inter = d.at("data.interaction");
    i.total(
      "reads_yesterday",
      inter.count("read_count").map(Value::from),
    );
    i.total(
      "readers_yesterday",
      inter.count("read_user_count").map(Value::from),
    );
    i.total(
      "interactions_yesterday",
      inter.count("interaction").map(Value::from),
    );
  }
  Ok(i)
}

pub async fn post(c: &Client, arg: &str) -> Result<Insights> {
  let v = posts::show(c, arg).await?;
  let mid = v.first_str(&["idstr", "mid"]).unwrap_or_default();
  let p = crate::parse::post(&v);
  let mut i = Insights {
    kind: "post".into(),
    subject: mid,
    title: p
      .as_ref()
      .and_then(|p| p.text.as_deref())
      .map(|t| social_core::text::truncate(&social_core::text::one_line(t), 60)),
    url: p.as_ref().and_then(|p| p.url.clone()),
    totals_period: Some("lifetime".into()),
    warnings: vec![NO_DATA_CENTER.into()],
    ..Insights::default()
  };
  i.total("likes", v.count("attitudes_count").map(Value::from));
  i.total("comments", v.count("comments_count").map(Value::from));
  i.total("reposts", v.count("reposts_count").map(Value::from));
  i.total("reads", v.count("reads_count").map(Value::from));
  i.total(
    "video_plays",
    v.first_count(&[
      "page_info.media_info.online_users_number",
      "page_info.play_count",
    ])
    .map(Value::from),
  );
  if let Some(n) = v.count("favorites_count").filter(|n| *n > 0) {
    i.total("favorites", Some(json!(n)));
  }
  Ok(i)
}

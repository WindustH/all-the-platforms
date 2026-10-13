//! Analytics from what weibo.com exposes: the account's lifetime counters and
//! yesterday's reads and interactions (profile detail), and a post's
//! counters. Weibo's creator data center (me.weibo.com) has no web API that
//! works outside its own pages, so trends and audience data are missing.

use social_core::{Insights, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::{posts, users};

const NO_DATA_CENTER: &str = "Weibo's creator data center (me.weibo.com) is not available here: \
   no daily trends or audience breakdowns, only the counters weibo.com shows";

pub async fn account(api: &Api) -> Result<Insights> {
  let uid = users::own_uid(api).await?;
  let info = api.get("/ajax/profile/info", &[("uid", &uid)]).await?;
  let user = info.at("data.user");
  let mut ins = Insights {
    kind: "account".into(),
    subject: uid.clone(),
    title: user.str("screen_name"),
    url: Some(crate::refs::user_url(&uid)),
    totals_period: Some("lifetime".into()),
    warnings: vec![NO_DATA_CENTER.into()],
    ..Insights::default()
  };
  ins.total("followers", user.count("followers_count").map(Value::from));
  ins.total("following", user.count("friends_count").map(Value::from));
  ins.total("posts", user.count("statuses_count").map(Value::from));
  for (metric, path) in [
    ("likes_received", "status_total_counter.like_cnt"),
    ("comments_received", "status_total_counter.comment_cnt"),
    ("reposts_received", "status_total_counter.repost_cnt"),
    ("interactions", "status_total_counter.total_cnt"),
  ] {
    ins.total(metric, user.count(path).map(Value::from));
  }
  if let Ok(d) = api.get("/ajax/profile/detail", &[("uid", &uid)]).await {
    let inter = d.at("data.interaction");
    ins.total(
      "reads_yesterday",
      inter.count("read_count").map(Value::from),
    );
    ins.total(
      "readers_yesterday",
      inter.count("read_user_count").map(Value::from),
    );
    ins.total(
      "interactions_yesterday",
      inter.count("interaction").map(Value::from),
    );
  }
  Ok(ins)
}

pub async fn post(api: &Api, arg: &str) -> Result<Insights> {
  let v = posts::show(api, arg).await?;
  let mid = v.first_str(&["idstr", "mid"]).unwrap_or_default();
  let post = crate::parse::post(&v);
  let mut ins = Insights {
    kind: "post".into(),
    subject: mid,
    title: post
      .as_ref()
      .and_then(|post| post.text.as_deref())
      .map(|t| social_core::text::truncate(&social_core::text::one_line(t), 60)),
    url: post.as_ref().and_then(|post| post.url.clone()),
    totals_period: Some("lifetime".into()),
    warnings: vec![NO_DATA_CENTER.into()],
    ..Insights::default()
  };
  ins.total("likes", v.count("attitudes_count").map(Value::from));
  ins.total("comments", v.count("comments_count").map(Value::from));
  ins.total("reposts", v.count("reposts_count").map(Value::from));
  ins.total("reads", v.count("reads_count").map(Value::from));
  ins.total(
    "video_plays",
    v.first_count(&[
      "page_info.media_info.online_users_number",
      "page_info.play_count",
    ])
    .map(Value::from),
  );
  if let Some(n) = v.count("favorites_count").filter(|n| *n > 0) {
    ins.total("favorites", Some(json!(n)));
  }
  Ok(ins)
}

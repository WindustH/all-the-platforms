//! `insights`: the counters TikTok shows publicly, for an account or a post.
//! TikTok Studio's analytics are not reachable through the web API used
//! here; ad accounts have reports in `social tiktok ads report`.

use social_core::{Insights, Result, Value};

use crate::api::Api;
use crate::{users, videos};

const STUDIO: &str = "TikTok Studio analytics (traffic sources, audience, watch time) are not \
   available through the web API; these are the public counters";

pub async fn insights(api: &Api, post: Option<&str>) -> Result<Insights> {
  match post {
    Some(p) => {
      let post = videos::read(api, p).await?;
      let mut i = Insights {
        kind: "post".into(),
        subject: post.id.clone(),
        title: post.text.clone(),
        url: post.url.clone(),
        totals_period: Some("lifetime".into()),
        ..Insights::default()
      };
      let m = &post.metrics;
      for (k, v) in [
        ("views", m.views),
        ("likes", m.likes),
        ("comments", m.comments),
        ("shares", m.shares),
        ("favorites", m.favorites),
        ("reposts", m.other.get("reposts").copied()),
      ] {
        i.total(k, v.map(Value::from));
      }
      if let (Some(views), Some(likes)) = (m.views, m.likes)
        && views > 0
      {
        i.total("like_rate", Some(Value::from(likes as f64 / views as f64)));
      }
      i.warnings.push(STUDIO.into());
      Ok(i)
    }
    None => {
      let me = users::whoami(api).await?;
      let mut i = Insights {
        kind: "account".into(),
        subject: me.handle.clone().unwrap_or_else(|| me.id.clone()),
        title: Some(me.name.clone()),
        url: me.url.clone(),
        totals_period: Some("lifetime".into()),
        ..Insights::default()
      };
      let s = &me.stats;
      for (k, v) in [
        ("followers", s.followers),
        ("following", s.following),
        ("posts", s.posts),
        ("likes", s.likes),
        ("friends", s.other.get("friends").copied()),
      ] {
        i.total(k, v.map(Value::from));
      }
      i.warnings.push(STUDIO.into());
      Ok(i)
    }
  }
}

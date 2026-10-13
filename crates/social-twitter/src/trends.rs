//! Trends of the Explore tabs (`GenericTimelineById`), shown as posts of
//! kind `trend` whose link is the matching search, and trends by place.

use social_core::{Collection, Page, PageReq, Post, Result, ValueExt, json};

use crate::api::{Api, REST};
use crate::graphql::EXPLORE_TIMELINE;
use crate::parse;
use crate::timeline::{self, Timeline, vars, with};

/// Explore tab timeline ids (padded base64 of `Timeline:` + a thrift tab name).
fn timeline_id(category: Option<&str>) -> &'static str {
  match category {
    Some("for-you") => "VGltZWxpbmU6DAC2CwABAAAAB2Zvcl95b3UAAA==",
    Some("news") => "VGltZWxpbmU6DAC2CwABAAAABG5ld3MAAA==",
    Some("sports") => "VGltZWxpbmU6DAC2CwABAAAABnNwb3J0cwAA",
    Some("entertainment") => "VGltZWxpbmU6DAC2CwABAAAADWVudGVydGFpbm1lbnQAAA==",
    _ => "VGltZWxpbmU6DAC2CwABAAAACHRyZW5kaW5nAAA=",
  }
}

pub async fn hot(api: &Api, category: Option<&str>, req: &PageReq) -> Result<Page<Post>> {
  api.require_login()?;
  let variables = with(
    vars(req),
    json!({
      "timelineId": timeline_id(category),
      "withQuickPromoteEligibilityTweetFields": true,
    }),
  );
  let paths = &["data.timeline.timeline.instructions"];
  timeline::page(
    api,
    &EXPLORE_TIMELINE,
    variables,
    paths,
    req,
    |tl: &Timeline| {
      tl.items()
        .filter_map(|(_, item)| parse::trend(item))
        .collect()
    },
  )
  .await
}

/// Places with trends (`woeid` for `trends --place`).
pub async fn places(api: &Api) -> Result<Page<Collection>> {
  api.require_login()?;
  let v = api
    .get(&format!("{REST}/trends/available.json"), &[])
    .await?;
  let items = v
    .as_array()
    .into_iter()
    .flatten()
    .filter_map(|p| {
      let mut c = Collection {
        id: p.str("woeid")?,
        kind: p
          .str("placeType.name")
          .unwrap_or_else(|| "place".into())
          .to_lowercase(),
        name: p.str("name").unwrap_or_default(),
        ..Collection::default()
      };
      if let Some(country) = p.str("country").filter(|c| !c.is_empty()) {
        c.extra.insert("country".into(), country.into());
      }
      Some(c)
    })
    .collect();
  Ok(Page::last(items))
}

/// The top trends of one place (`1` is worldwide), with tweet volumes.
pub async fn place_trends(api: &Api, woeid: &str) -> Result<Page<Post>> {
  api.require_login()?;
  let v = api
    .get(&format!("{REST}/trends/place.json"), &[("id", woeid)])
    .await?;
  let block = v.at("0");
  let items = block
    .list("trends")
    .iter()
    .enumerate()
    .filter_map(|(i, t)| {
      let name = t.str("name")?;
      let mut post = Post {
        kind: "trend".into(),
        title: Some(name.clone()),
        url: t.str("url"),
        raw: Some(t.clone()),
        id: name,
        ..Post::default()
      };
      if let Some(volume) = t.u64("tweet_volume") {
        post.metrics.other.insert("tweets".into(), volume);
      }
      post.extra.insert("rank".into(), (i as u64 + 1).into());
      Some(post)
    })
    .collect();
  Ok(Page::last(items))
}

//! Administrative districts (`/ws/district/v1/*`): one list of every
//! province, city and district, walked from the one asked for; and
//! turning a region name into its adcode.

use atp_core::{Collection, Error, Page, Result, Value, ValueExt, json};

use crate::api::Api;
use crate::refs;

const LEVELS: &[&str] = &["province", "city", "district"];

/// Districts called `name` (`中国` for the provinces) and the levels
/// below them, `depth` levels deep.
pub async fn districts(api: &Api, name: &str, depth: u8) -> Result<Page<Collection>> {
  let v = api.get("/ws/district/v1/list", &[]).await?;
  let levels: Vec<&[Value]> = v
    .list("result")
    .iter()
    .map(|l| l.as_array().map_or(&[][..], Vec::as_slice))
    .collect();
  let mut out = Vec::new();
  if matches!(name.trim(), "中国" | "全国" | "China" | "china") {
    for i in 0..levels.first().map_or(0, |l| l.len()) {
      walk(&levels, 0, i, None, depth.saturating_sub(1), &mut out);
    }
  } else {
    let (level, index) = find(&levels, name.trim())
      .ok_or_else(|| Error::not_found(format!("Tencent has no district called `{name}`")))?;
    walk(&levels, level, index, None, depth, &mut out);
  }
  Ok(Page::last(out))
}

/// The first district whose adcode, short name or full name is `name`.
fn find(levels: &[&[Value]], name: &str) -> Option<(usize, usize)> {
  levels.iter().enumerate().find_map(|(level, items)| {
    let hit = |d: &Value| {
      [d.str("id"), d.str("name"), d.str("fullname")]
        .iter()
        .any(|x| x.as_deref() == Some(name))
    };
    items.iter().position(hit).map(|index| (level, index))
  })
}

fn walk(
  levels: &[&[Value]],
  level: usize,
  index: usize,
  parent: Option<&str>,
  depth: u8,
  out: &mut Vec<Collection>,
) {
  let Some(d) = levels.get(level).and_then(|l| l.get(index)) else {
    return;
  };
  let id = d.str("id").unwrap_or_default();
  let mut c = Collection {
    id: id.clone(),
    kind: "district".into(),
    name: d.first_str(&["fullname", "name"]).unwrap_or_default(),
    description: LEVELS.get(level).map(|l| (*l).to_owned()),
    raw: Some(d.clone()),
    ..Collection::default()
  };
  if let Some(center) = refs::located(d.at("location")) {
    c.extra.insert("center".into(), json!(center));
  }
  if let Some(parent) = parent {
    c.extra.insert("parent".into(), parent.into());
  }
  out.push(c);
  let (Some(from), Some(to)) = (d.u64("cidx.0"), d.u64("cidx.1")) else {
    return;
  };
  if depth == 0 {
    return;
  }
  for child in from as usize..=to as usize {
    walk(levels, level + 1, child, Some(&id), depth - 1, out);
  }
}

/// The adcode of a region: six digits as given, or the first district
/// matching the name.
pub async fn adcode(api: &Api, region: &str) -> Result<String> {
  let region = region.trim();
  if region.len() == 6 && region.bytes().all(|b| b.is_ascii_digit()) {
    return Ok(region.to_owned());
  }
  let v = api
    .get("/ws/district/v1/search", &[("keyword", region.to_owned())])
    .await?;
  v.str("result.0.0.id")
    .ok_or_else(|| Error::not_found(format!("Tencent has no city or district called `{region}`")))
}

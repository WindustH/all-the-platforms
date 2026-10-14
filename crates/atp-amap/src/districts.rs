//! Administrative districts (`/v3/config/district`) and turning a region
//! name into the adcode that weather and traffic take.

use atp_core::{Collection, Error, Page, Result, Value, ValueExt};

use crate::api::Api;
use crate::parse;

async fn query(api: &Api, keywords: &str, depth: u8) -> Result<Value> {
  let params = [
    ("keywords", keywords.to_owned()),
    ("subdistrict", depth.min(3).to_string()),
    ("extensions", "base".to_owned()),
  ];
  api.get("/v3/config/district", &params).await
}

/// Districts called `name` (the whole country when empty) and the levels
/// below them, `depth` levels deep.
pub async fn districts(api: &Api, name: &str, depth: u8) -> Result<Page<Collection>> {
  let v = query(api, name, depth).await?;
  let mut out = Vec::new();
  for district in v.list("districts") {
    flatten(district, None, &mut out);
  }
  if out.is_empty() {
    return Err(Error::not_found(format!(
      "Amap has no district called `{name}`"
    )));
  }
  Ok(Page::last(out))
}

fn flatten(v: &Value, parent: Option<&str>, out: &mut Vec<Collection>) {
  let Some(district) = parse::district(v, parent) else {
    return;
  };
  let id = district.id.clone();
  out.push(district);
  for child in v.list("districts") {
    flatten(child, Some(&id), out);
  }
}

/// The adcode of a region: six digits as given, or the first district
/// matching the name.
pub async fn adcode(api: &Api, region: &str) -> Result<String> {
  let region = region.trim();
  if region.len() == 6 && region.bytes().all(|b| b.is_ascii_digit()) {
    return Ok(region.to_owned());
  }
  let v = query(api, region, 0).await?;
  v.str("districts.0.adcode")
    .ok_or_else(|| Error::not_found(format!("Amap has no city or district called `{region}`")))
}

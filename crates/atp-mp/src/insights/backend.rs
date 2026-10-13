//! Account analytics from the backend: the follower series of
//! `misc/useranalysis` and the content report of `misc/appmsganalysis`
//! (fields only partly documented: best effort).

use atp_core::{Insights, Result, Value, ValueExt};
use jiff::civil::Date;

use super::{Daily, add, num};
use crate::api::Api;

pub async fn followers(api: &Api, from: Date, to: Date, daily: &mut Daily) -> Result<()> {
  let v = api
    .backend()
    .get(
      "/misc/useranalysis",
      vec![
        ("begin_date", from.to_string()),
        ("end_date", to.to_string()),
        ("source", "99999999".into()),
      ],
    )
    .await?;
  let categories = v.list("category_list");
  let all = categories
    .iter()
    .find(|c| c.i64("user_source") == Some(99999999))
    .or_else(|| categories.first());
  for row in all.map(|c| c.list("list")).unwrap_or_default() {
    let date = row.str("date").unwrap_or_default();
    add(daily, &date, "new_followers", num(row, "new_user"));
    add(daily, &date, "lost_followers", num(row, "cancel_user"));
    add(daily, &date, "net_followers", num(row, "netgain_user"));
    add(daily, &date, "followers", num(row, "cumulate_user"));
  }
  Ok(())
}

/// The backend's content report: daily rows per metric plus summaries for
/// yesterday / 7 / 30 days.
pub async fn content(api: &Api, daily: &mut Daily, ins: &mut Insights) -> Result<()> {
  let v = api
    .backend()
    .get("/misc/appmsganalysis", vec![("action", "report".into())])
    .await?;
  let mut found = false;
  for (metric, key) in [
    ("views", "read"),
    ("shares", "share"),
    ("likes", "like"),
    ("zaikan", "zaikan"),
    ("comments", "comment"),
  ] {
    let rows = v.list(&format!("{key}_item"));
    // Rows per day and scene; 9999 (or a missing scene) is the total.
    let totals: Vec<&Value> = rows
      .iter()
      .filter(|r| matches!(r.i64("attr_scene"), None | Some(9999)))
      .collect();
    let rows: Vec<&Value> = if totals.is_empty() {
      rows.iter().collect()
    } else {
      totals
    };
    for r in rows {
      let date = r.str("ref_date").unwrap_or_default();
      add(daily, &date, metric, r.f64("uv").or_else(|| r.f64("pv")));
      found = true;
    }
    let summary = v.list(&format!("{key}_summary"));
    if !summary.is_empty() {
      let periods: Vec<Value> = summary.iter().map(|s| s.at("data_value").clone()).collect();
      ins
        .extra
        .insert(format!("{metric}_yesterday_7d_30d"), Value::from(periods));
      found = true;
    }
  }
  if !found {
    ins
      .warnings
      .push("the backend's content report had no data this CLI understands".into());
  }
  Ok(())
}

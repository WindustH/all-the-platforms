//! Flight fares as posts: one per day of the fare calendar, with the day's
//! lowest one-way fare in `price` (with taxes and fees in its label).

use atp_core::text::from_beijing;
use atp_core::{Post, Price, Value, ValueExt};

use super::put;
use crate::refs;

/// A day of the fare calendar (`PriceList` entry) between two airport
/// cities (`from`, `to`: their codes), titled with `route` (`上海 → 北京`).
pub fn fare(v: &Value, from: &str, to: &str, route: &str) -> Option<Post> {
  let day = v.str("DepartDate")?.get(..10)?.to_owned();
  let amount = v.f64("Price").filter(|p| *p > 0.0)?;
  let label = match v.f64("TotalPrice").filter(|t| *t > amount) {
    Some(total) => format!("起，含税费 ¥{total}"),
    None => "起".into(),
  };
  let mut post = Post {
    id: format!("{from}-{to}-{day}"),
    kind: "fare".into(),
    title: Some(format!("{route} {day}")),
    text: v.str("DirectCalendarText"),
    url: Some(refs::flights_url(from, to, &day)),
    created_at: from_beijing(&day),
    price: Some(Price {
      label: Some(label),
      ..Price::cny(amount)
    }),
    raw: Some(v.clone()),
    ..Post::default()
  };
  put(&mut post.extra, "date", Some(day.into()));
  if v.i64("Flag") == Some(1) {
    put(&mut post.extra, "lowest", Some(true.into()));
  }
  Some(post)
}

//! Messaging analytics of the business account: messages sent and
//! delivered, and conversation volume and cost by pricing category.

use std::collections::BTreeMap;

use social_core::text::from_secs;
use social_core::{Breakdown, Ctx, Insights, Point, Result, Series, Share, Value, ValueExt, json};

use super::{Cloud, day};

impl Cloud {
  /// Messages sent / delivered per day, and pricing analytics per category.
  pub async fn insights(&self, ctx: &Ctx, days: u32) -> Result<Insights> {
    let waba = self.waba()?.to_owned();
    let end = jiff::Timestamp::now().as_second();
    let start = end - i64::from(days.clamp(1, 90)) * 86_400;
    let mut ins = Insights {
      kind: "account".into(),
      subject: self.phone.clone(),
      title: Some("WhatsApp Business messaging".into()),
      from: from_secs(start).map(|t| day(&t)),
      to: from_secs(end).map(|t| day(&t)),
      totals_period: Some(format!("{days}d")),
      ..Insights::default()
    };
    if let Err(e) = self.add_messages(ctx, &waba, start, end, &mut ins).await {
      ins
        .warnings
        .push(format!("message analytics: {}", e.message));
    }
    if let Err(e) = self.add_pricing(ctx, &waba, start, end, &mut ins).await {
      ins
        .warnings
        .push(format!("pricing analytics: {}", e.message));
    }
    Ok(ins)
  }

  /// Messages sent and delivered per day.
  async fn add_messages(
    &self,
    ctx: &Ctx,
    waba: &str,
    start: i64,
    end: i64,
    ins: &mut Insights,
  ) -> Result<()> {
    let fields = format!("analytics.start({start}).end({end}).granularity(DAY)");
    let v = self.get(ctx, waba, &[("fields", fields)]).await?;
    let points = v.list("analytics.data_points");
    for metric in ["sent", "delivered"] {
      let series: Vec<Point> = points
        .iter()
        .filter_map(|p| {
          let date = from_secs(p.i64("start")?).map(|t| day(&t))?;
          Some(Point {
            date,
            value: p.u64(metric).unwrap_or(0).into(),
          })
        })
        .collect();
      let total: u64 = points.iter().filter_map(|p| p.u64(metric)).sum();
      ins.total(&format!("messages_{metric}"), Some(Value::from(total)));
      ins.series.push(Series {
        metric: format!("messages_{metric}"),
        points: series,
      });
    }
    Ok(())
  }

  /// Conversation volume by pricing category, and the cost.
  async fn add_pricing(
    &self,
    ctx: &Ctx,
    waba: &str,
    start: i64,
    end: i64,
    ins: &mut Insights,
  ) -> Result<()> {
    let fields = format!(
      "pricing_analytics.start({start}).end({end}).granularity(DAILY).dimensions([\"PRICING_CATEGORY\"])"
    );
    let v = self.get(ctx, waba, &[("fields", fields)]).await?;
    let mut volume: BTreeMap<String, u64> = BTreeMap::new();
    let mut cost = 0.0;
    for d in v.list("pricing_analytics.data") {
      for p in d.list("data_points") {
        let category = p.str("pricing_category").unwrap_or_else(|| "other".into());
        *volume.entry(category.to_lowercase()).or_default() += p.u64("volume").unwrap_or(0);
        cost += p.f64("cost").unwrap_or(0.0);
      }
    }
    ins.total("cost", Some(json!(cost)));
    ins.breakdowns.push(Breakdown {
      dimension: "pricing_category".into(),
      items: volume
        .into_iter()
        .map(|(label, n)| Share {
          label,
          value: n.into(),
          ..Share::default()
        })
        .collect(),
      ..Breakdown::default()
    });
    Ok(())
  }
}

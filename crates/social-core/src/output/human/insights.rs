//! Analytics: totals, series as sparklines, breakdowns.

use super::*;

/// Integers as counts, `*_rate` / `*ratio` / `ctr` as percentages, other decimals rounded.
pub(super) fn metric(name: &str, v: &serde_json::Value) -> String {
  let Some(n) = v.as_f64() else {
    return crate::output::rows::cell(v);
  };
  if name.ends_with("rate") || name.ends_with("ratio") || name == "ctr" || name.ends_with("_ctr") {
    return format!("{:.1}%", n * 100.0);
  }
  match v.as_u64() {
    Some(u) => fmt_count(u),
    None => format!("{n:.2}")
      .trim_end_matches('0')
      .trim_end_matches('.')
      .to_owned(),
  }
}

pub(super) fn insights(i: &Insights) {
  let title = i
    .title
    .clone()
    .unwrap_or_else(|| format!("{} insights", i.kind));
  println!("{}", title.if_supports_color(Stream::Stdout, |t| t.bold()));
  let mut meta = vec![i.kind.clone(), i.subject.clone()];
  if let (Some(from), Some(to)) = (&i.from, &i.to) {
    meta.push(format!("{from} → {to}"));
  }
  println!(
    "{}",
    meta
      .join(" · ")
      .if_supports_color(Stream::Stdout, |t| t.dimmed())
  );
  if let Some(url) = &i.url {
    println!("{}", url.if_supports_color(Stream::Stdout, |t| t.cyan()));
  }
  for w in &i.warnings {
    println!(
      "{} {w}",
      "!".if_supports_color(Stream::Stdout, |t| t.yellow())
    );
  }
  if !i.totals.is_empty() {
    let period = i.totals_period.as_deref().map(|p| format!("Value ({p})"));
    let mut t = table(&["Metric", period.as_deref().unwrap_or("Value")]);
    for (k, v) in &i.totals {
      t.add_row(vec![
        Cell::new(k),
        Cell::new(metric(k, v)).set_alignment(CellAlignment::Right),
      ]);
    }
    println!("\n{t}");
  }
  if !i.series.is_empty() {
    // Dates down, metrics across; the most recent two weeks.
    let mut dates: Vec<&str> = i
      .series
      .iter()
      .flat_map(|s| s.points.iter().map(|p| p.date.as_str()))
      .collect();
    dates.sort_unstable();
    dates.dedup();
    let recent = &dates[dates.len().saturating_sub(14)..];
    let mut header = vec!["Date"];
    header.extend(i.series.iter().map(|s| s.metric.as_str()));
    let mut t = table(&header);
    for d in recent {
      let mut row = vec![Cell::new(d)];
      for s in &i.series {
        let v = s
          .points
          .iter()
          .find(|p| p.date == *d)
          .map(|p| metric(&s.metric, &p.value));
        row.push(Cell::new(v.unwrap_or_default()).set_alignment(CellAlignment::Right));
      }
      t.add_row(row);
    }
    println!("\n{t}");
    if dates.len() > recent.len() {
      note(&format!(
        "(last {} of {} days; --format csv for all)",
        recent.len(),
        dates.len()
      ));
    }
  }
  for b in &i.breakdowns {
    let title = match &b.period {
      Some(p) => format!("{} ({p})", b.dimension),
      None => b.dimension.clone(),
    };
    let mut t = table(&[title.as_str(), "Value", "Share", ""]);
    for x in &b.items {
      let ratio = x
        .ratio
        .map(|r| format!("{:.1}%", r * 100.0))
        .unwrap_or_default();
      let bar = "█".repeat((x.ratio.unwrap_or(0.0) * 20.0).round() as usize);
      t.add_row(vec![
        Cell::new(&x.label),
        Cell::new(metric("", &x.value)).set_alignment(CellAlignment::Right),
        Cell::new(ratio).set_alignment(CellAlignment::Right),
        Cell::new(bar).add_attribute(Attribute::Dim),
      ]);
    }
    println!("\n{t}");
  }
}

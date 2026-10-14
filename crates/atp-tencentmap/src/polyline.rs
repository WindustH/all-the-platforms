//! Route polylines as Tencent compresses them: `[lat, lng, Δlat, Δlng, …]`,
//! the first point whole and every later number the difference from the
//! number two places before, in millionths of a degree.

/// The points as `(lng, lat)`.
pub fn decompress(numbers: &[f64]) -> Vec<(f64, f64)> {
  let mut coords = numbers.to_vec();
  for i in 2..coords.len() {
    coords[i] = coords[i - 2] + coords[i] / 1_000_000.0;
  }
  let (pairs, _) = coords.as_chunks::<2>();
  pairs
    .iter()
    .map(|[lat, lng]| (round6(*lng), round6(*lat)))
    .collect()
}

fn round6(x: f64) -> f64 {
  (x * 1e6).round() / 1e6
}

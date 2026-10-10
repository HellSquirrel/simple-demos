//! The smoke column above the flame: its edge is scalloped. Period of the scallops along the plume (rows 10..90),
//! from the soot field's half-width per row, plus how far soot survives above the luminous zone. SUBDT picks the step.
use flame::{Sim, W};
fn main() {
    let mut s = Sim::new();
    if let Ok(v) = std::env::var("SUBDT") { s.set_substep(1.0 / v.parse::<f32>().unwrap()); }
    if let Ok(v) = std::env::var("SOX") { s.set_oxidation(v.parse().unwrap(), 11000.0, 1700.0); }
    let env = |k: &str, d: f32| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
    if let Ok(v) = std::env::var("PIT") { s.set_solver(v.parse().unwrap()); }
    if std::env::var("RK2").is_ok() { s.set_rk2(true); }
    s.set_wick(0.0, 0.0); s.set_room(0.209, 1.0, env("SWIRL", 1.0), 0.0); s.strike(false);
    for _ in 0..480 { s.step(1.0 / 60.0); }
    let (so, t) = (s.field_s(), s.field_t());
    // half-width of the soot column per row (cells where S > 0.05), rows 10..90, and the hot column's width (T > 600 K)
    let width = |j: usize, f: &dyn Fn(usize) -> bool| -> f32 { let (mut lo, mut hi) = (W, 0); for i in 0..W { if f(i + j * W) { lo = lo.min(i); hi = hi.max(i); } } if hi >= lo { (hi - lo + 1) as f32 } else { 0.0 } };
    let ws: Vec<f32> = (10..=90).map(|j| width(j, &|k| so[k] > 0.05)).collect();
    let det: Vec<f32> = (0..ws.len()).map(|i| { let (lo, hi) = (i.saturating_sub(6), (i + 6).min(ws.len() - 1)); ws[i] - ws[lo..=hi].iter().sum::<f32>() / (hi - lo + 1) as f32 }).collect();
    let ac = |lag: usize| det.iter().zip(det.iter().skip(lag)).map(|(a, b)| a * b).sum::<f32>();
    let (mut period, mut best) = (0usize, 0f32);
    for lag in 4..40 { let v = ac(lag); if v > best && ac(lag - 1) <= v && v >= ac(lag + 1) { best = v; period = lag; } }
    let amp = (det.iter().map(|v| v * v).sum::<f32>() / det.len() as f32).sqrt();
    // soot above the luminous zone: topmost row with S > 0.05 vs topmost row with T > 1300 K and S > 0.05 (glowing soot)
    let top = |f: &dyn Fn(usize) -> bool| (0..280usize).find(|&j| (0..W).any(|i| f(i + j * W))).unwrap_or(280);
    let (smoke_top, glow_top) = (top(&|k| so[k] > 0.05), top(&|k| so[k] > 0.05 && t[k] > 1300.0));
    let smax: f32 = (0..W * 60).map(|k| so[k]).fold(0.0, f32::max); // in the top 15 mm
    println!("scallops: period {} cells = {:.1} mm, amplitude {:.1} cells; soot column {:.1} mm wide at mid-plume; smoke reaches {:.1} mm above the glowing soot; max soot in the top 15 mm {:.3}",
        period, period as f32 * 0.25, amp, ws[ws.len() / 2] * 0.25, (glow_top as f32 - smoke_top as f32) * 0.25, smax);
}

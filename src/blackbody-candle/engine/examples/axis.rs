//! Profiles along the flame axis after 8 s: is structure in the soot only, or in temperature and flow too?
use flame::{Sim, W};
fn main() {
    let mut s = Sim::new();
    // the wick model from the environment, like tools/snapshot.js: TIPR, TVAP, VMIX
    let env = |k: &str, d: f32| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
    s.set_wick_model(env("TIPR", 14.0), env("TVAP", 700.0), env("VMIX", 0.0));
    s.set_vapour(env("KF", 0.0));
    s.strike(false);
    for _ in 0..480 { s.step(1.0 / 60.0); }
    let (t, f, so, v) = (s.field_t(), s.field_f(), s.field_s(), s.field_v());
    // band metric: rms of the second difference of T along the axis over 5-cell steps (smooth core -> small)
    let (mut acc, mut n) = (0f64, 0usize);
    // the flame's centre column: the temperature-weighted centre of row 130
    let (mut wsum, mut wc) = (0f32, 0f32);
    for i in 0..W { let x = (t[i + 130 * W] - 300.0).max(0.0); wsum += x; wc += x * i as f32; }
    let c = if wsum > 0.0 { (wc / wsum).round() as usize } else { 84 };
    for j in 105..=150 { let k = c + j * W; let d = t[k - 5 * W] - 2.0 * t[k] + t[k + 5 * W]; acc += (d * d) as f64; n += 1; }
    let core: Vec<f32> = (110..=150).map(|j| t[c + j * W]).collect();
    println!("column {c}: band metric {:.0} K; core T rows 110..150: mean {:.0} K, min {:.0}, max {:.0}", (acc / n as f64).sqrt(), core.iter().sum::<f32>() / core.len() as f32, core.iter().cloned().fold(f32::MAX, f32::min), core.iter().cloned().fold(0.0, f32::max));
    println!("row    T[K]     fuel     soot    v[cells/s]   (column {c}; wick tip row 156)");
    for j in (96..=158).rev().step_by(4) {
        let k = c + j * W;
        println!("{j:3}  {:6.0}  {:7.4}  {:7.4}  {:9.0}", t[k], f[k], so[k], v[k]);
    }
    println!("row 130 across the flame, columns 68..108: T / fuel / soot");
    let (o2, so2) = (s.field_f(), s.field_s());
    println!("  T    {}", (68..=108).step_by(2).map(|i| format!("{:4.0}", t[i + 130 * W])).collect::<Vec<_>>().join(" "));
    println!("  F    {}", (68..=108).step_by(2).map(|i| format!("{:4.2}", o2[i + 130 * W])).collect::<Vec<_>>().join(" "));
    println!("  S    {}", (68..=108).step_by(2).map(|i| format!("{:4.2}", so2[i + 130 * W])).collect::<Vec<_>>().join(" "));
}

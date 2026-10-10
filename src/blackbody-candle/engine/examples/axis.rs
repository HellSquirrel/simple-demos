//! Flame structure after 8 s along the flame's own centreline (the temperature-weighted centre of each row).
//! Knobs from the environment, like tools/snapshot.js: TIPR TVAP VMIX KF AS FUEL ENTMODE SOOT SG, plus DRAFT and CURL
//! (both default 0 here, for a straight flame).
use flame::{Sim, W};
fn main() {
    let env = |k: &str, d: f32| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
    let mut s = Sim::new();
    s.set_wick_model(env("TIPR", 14.0), env("TVAP", 700.0), env("VMIX", 0.0));
    s.set_vapour(env("KF", 0.0));
    s.set_chem(env("AS", 1.0), 1.0);
    s.set_fuel(env("FUEL", 0.5));
    s.set_soot(env("SOOT", 1.0));
    s.set_growth(env("SG", 0.0));
    if std::env::var("INC").is_ok() || std::env::var("ESF").is_ok() { s.set_inception(env("INC", 0.5), env("ESF", 20000.0)); }
    s.set_entrain_mode(env("ENTMODE", 0.0) as u32);
    s.set_wick(env("CURL", 0.0), 0.0);
    s.set_room(0.209, 1.0, 1.0, env("DRAFT", 0.0));
    if let Ok(v) = std::env::var("SUBDT") { s.set_substep(1.0 / v.parse::<f32>().unwrap()); }
    if let Ok(v) = std::env::var("CHEMSUB") { s.set_chem_sub(v.parse().unwrap()); }
    if std::env::var("STRANG").is_ok() { s.set_strang(true); }
    if let Ok(v) = std::env::var("DIFFSUB") { s.set_diff_sub(v.parse().unwrap()); }
    s.strike(false);
    for _ in 0..480 { s.step(1.0 / 60.0); }
    if s.phase_code() != 1 { println!("OUT"); return; }
    let (t, f, o, so) = (s.field_t(), s.field_f(), s.field_o(), s.field_s());
    let centre = |j: usize| -> usize { let (mut ws, mut wc) = (0f32, 0f32); for i in 0..W { let x = (t[i + j * W] - 300.0).max(0.0); ws += x; wc += x * i as f32; } if ws > 0.0 { (wc / ws).round() as usize } else { 84 } };
    // fuel core: rows (from the wick tip, 156, upward) where the centreline fuel is above 0.05
    let mut core_top = 156usize; let mut core_s = 0f32; let mut core_t_max = 0f32; let mut n = 0;
    for j in (60..=156).rev() { let c = centre(j); let k = c + j * W; if f[k] > 0.05 { core_top = j; core_s += so[k]; core_t_max = core_t_max.max(t[k]); n += 1; } else { break; } }
    let mut tmax = 0f32; for k in 0..t.len() { tmax = tmax.max(t[k]); }
    let mut smax = 0f32; for k in 0..so.len() { smax = smax.max(so[k]); }
    // banding along the centreline, rows 96..150: amplitude (rms of S about a 9-cell running mean) and the dominant period
    // (first peak of the autocorrelation of the detrended S), in cells
    let line: Vec<f32> = (96..=150).map(|j| so[centre(j) + j * W]).collect();
    let det: Vec<f32> = (0..line.len()).map(|i| { let (lo, hi) = (i.saturating_sub(4), (i + 4).min(line.len() - 1)); line[i] - line[lo..=hi].iter().sum::<f32>() / (hi - lo + 1) as f32 }).collect();
    let amp = (det.iter().map(|v| v * v).sum::<f32>() / det.len() as f32).sqrt();
    let ac = |lag: usize| det.iter().zip(det.iter().skip(lag)).map(|(a, b)| a * b).sum::<f32>();
    let (mut period, mut best) = (0usize, 0f32);
    for lag in 3..25 { let v = ac(lag); if v > best && ac(lag - 1) <= v && v >= ac(lag + 1) { best = v; period = lag; } }
    println!("soot bands on the centreline: amplitude {amp:.3} (mean S {:.3}), dominant period {} cells", line.iter().sum::<f32>() / line.len() as f32, if period > 0 { period.to_string() } else { "none".into() });
    println!("fuel core {:.1} mm tall (rows {}..156), centreline T max in it {:.0} K, mean soot in it {:.3}; hottest gas {:.0} K, most soot anywhere {:.2}", (156 - core_top) as f32 * 0.25, core_top, core_t_max, if n > 0 { core_s / n as f32 } else { 0.0 }, tmax, smax);
    if std::env::var("TABLE").is_ok() {
        println!(" row  col   T[K]   fuel    O2   soot");
        for j in (88..=156).rev().step_by(8) { let c = centre(j); let k = c + j * W; println!(" {j:3}  {c:3}  {:5.0}  {:5.2}  {:4.2}  {:5.3}", t[k], f[k], o[k], so[k]); }
    }
}

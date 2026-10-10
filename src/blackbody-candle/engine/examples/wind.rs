//! Does the room wind reach the flame? Time series of the wind state and of the horizontal velocity 5 mm above the
//! wick, as standard deviations in mm/s, at the default draft. SUBDT=120|240 picks the substep.
use flame::{Sim, W};
fn main() {
    let env = |k: &str, d: f32| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
    let mut s = Sim::new();
    if let Ok(v) = std::env::var("SUBDT") { s.set_substep(1.0 / v.parse::<f32>().unwrap()); }
    s.set_room(0.209, 1.0, 1.0, env("DRAFT", 1.0));
    s.strike(false);
    for _ in 0..180 { s.step(1.0 / 60.0); }
    let (mut d, mut u1, mut u2) = (vec![], vec![], vec![]);
    for _ in 0..3600 {
        s.step(1.0 / 60.0);
        d.push(s.draft_state() * 0.25); // cells/s -> mm/s
        let u = s.field_u();
        u1.push(u[84 + 136 * W] * 0.25); // 5 mm above the wick tip, on the axis
        u2.push(u[20 + 136 * W] * 0.25); // same height, 16 mm to the left: open air
    }
    let sd = |v: &Vec<f32>| { let m = v.iter().sum::<f32>() / v.len() as f32; (v.iter().map(|x| (x - m) * (x - m)).sum::<f32>() / v.len() as f32).sqrt() };
    println!("wind state σ {:.0} mm/s; u at the flame σ {:.0} mm/s; u in open air σ {:.0} mm/s", sd(&d), sd(&u1), sd(&u2));
}

//! Native per-stage profile of the engine: cargo run --release --example prof
fn main() {
    let mut s = flame::Sim::new();
    s.strike();
    for _ in 0..240 { s.step(1.0 / 60.0); }
    for (k, v) in s.profile(200) { println!("{k:40} {v:.3} ms"); }
}

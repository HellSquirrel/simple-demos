//! Native per-stage profile of the engine: cargo run --release --example prof
use std::time::Instant;
fn main() {
    let mut s = flame::Sim::new();
    let mut r = flame::Renderer::new();
    s.strike();
    for _ in 0..240 { s.step(1.0 / 60.0); }
    for (k, v) in s.profile(200) { println!("{k:40} {v:.3} ms"); }
    let t0 = Instant::now();
    for _ in 0..200 { r.render(&s); }
    println!("{:40} {:.3} ms", "render, per frame", t0.elapsed().as_secs_f64() * 1e3 / 200.0);
}

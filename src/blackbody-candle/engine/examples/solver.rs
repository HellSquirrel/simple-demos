//! How converged are the two iterative solvers? Run: cargo run --release --example solver
use flame::Sim;
use std::time::Instant;
fn main() {
    let mut s = Sim::new();
    s.strike(false);
    for _ in 0..240 { s.step(1.0 / 60.0); }
    println!("pressure: residual divergence after n sweeps, each from the same state after 4 s of burning");
    let before = s.divergence_rms();
    println!("  sweeps     0  residual {:.4}  (1.00x)", before);
    for &n in &[8usize, 16, 32, 64, 128, 256, 1024] {
        let mut c = s.clone();
        c.set_solver(n);
        let t0 = Instant::now();
        c.project();
        let ms = t0.elapsed().as_secs_f64() * 1e3;
        let after = c.divergence_rms();
        println!("  sweeps {:>5}  residual {:.4}  ({:.2}x)  {:.2} ms", n, after, after / before, ms);
    }
}

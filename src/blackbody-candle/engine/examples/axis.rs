//! Profiles along the flame axis after 8 s: is structure in the soot only, or in temperature and flow too?
use flame::{Sim, W};
fn main() {
    let mut s = Sim::new();
    s.strike(false);
    for _ in 0..480 { s.step(1.0 / 60.0); }
    let (t, f, so, v) = (s.field_t(), s.field_f(), s.field_s(), s.field_v());
    println!("row    T[K]     fuel     soot    v[cells/s]   (column 84, the wick axis; wick tip at row 156)");
    for j in (96..=158).rev() {
        let k = 84 + j * W;
        println!("{j:3}  {:6.0}  {:7.4}  {:7.4}  {:9.0}", t[k], f[k], so[k], v[k]);
    }
}

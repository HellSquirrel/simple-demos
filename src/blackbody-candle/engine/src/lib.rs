//! Blackbody Candle: a reacting, buoyant 2D flow of a candle flame, rendered with
//! Planck's law through the CIE 1931 observer. Compiled to wasm32 with a plain C ABI:
//! JS calls the exported functions and reads results straight out of linear memory.
#![allow(static_mut_refs, clippy::too_many_arguments)]

use std::f32::consts::PI;

// ---------------------------------------------------------------- grid & physics constants
pub const W: usize = 168;
pub const H: usize = 280;
pub const N: usize = W * H;
const DX: f32 = 0.25; // mm per cell (42 x 70 mm window)
const CELL_AREA: f32 = DX * DX / 0.25; // cell area relative to the original 0.5 mm grid: keeps summed light and heat comparable
const TA: f32 = 300.0; // room temperature, K
const G: f32 = 9810.0 / DX; // gravity, cells/s^2
const A: f32 = 1e5; // one-step chemistry: pre-exponential, 1/s
const EA: f32 = 6000.0; // activation temperature, K
const STO: f32 = 6.0; // oxygen used per unit fuel
const Q: f32 = 11300.0; // temperature rise per unit fuel burned, K: with STO = 6 a stoichiometric mixture reaches ~2,200 K, the adiabatic flame temperature of hydrocarbons in air
const SF: f32 = 1.45e7; // soot inception pre-exponential, set so the rate at 1,750 K equals the earlier calibration (1.5e5 at 12,000 K)
const ESF: f32 = 20000.0; // soot inception activation temperature, K: soot models use ~21,000, which keeps inception in the hot sheath and the vapour core dark
const SO: f32 = 1e6; // soot oxidation
const ESO: f32 = 11000.0;
const KRAD: f32 = 5e-9; // optically thin radiative loss
const ALPHA0: f32 = 22.0; // thermal diffusivity of air at 300 K, mm^2/s
const D_EXP: f32 = 1.7; // gas diffusivities scale as (T/300)^1.7 (kinetic theory with the viscosity exponent); ~400 mm^2/s in the flame
const PR: f32 = 0.71; // Prandtl number of air: kinematic viscosity = PR * alpha
const LE_F: f32 = 3.0; // Lewis number of the fuel vapour: heavy hydrocarbons diffuse about 3x slower than heat
const LE_O: f32 = 1.0; // oxygen diffuses about like heat
const OHP: f32 = 0.012; // OH proxy for soot oxidation
const ENT: f32 = 0.25; // out-of-plane entrainment
const ENT_R: f32 = 20.0; // flame radius the entrainment rate is scaled by, cells (5 mm)
const EXPAND: f32 = 0.5; // thermal expansion in the pressure solve
const FUEL: f32 = 0.5; // wax vapour supply of the wick, in fuel-fraction units; sets the flame size
const WICK_CURL: f32 = 6.0; // a braided wick curls so its tip burns in the sheath: the vapour source sits this many cells to the side of the axis
const SUPPLY_SIGMA: f32 = 0.12; // the capillary feed and melt pool fluctuate: relative noise on the vapour supply
const SUPPLY_TAU: f32 = 0.3; // and its correlation time, s
const DRAFT_SIGMA: f32 = 60.0; // room air: standard deviation of the wandering wind, mm/s, at draft 1x (a normally ventilated room)
const DRAFT_TAU: f32 = 0.8; // its correlation time, s
const DRAFT_COUPLING: f32 = 3.0; // rate at which the air in the window follows the room wind, 1/s: about the rate the wind crosses the 42 mm window
const HEAD_X: usize = 84; // wick column
const HEAD0: f32 = 156.0; // wick tip row: the flame anchors here and never moves
const WAX_TOP: usize = 174; // first row of solid wax below the wick
const WAX_HALF: usize = 20; // half-width of the wax column, cells (10 mm across)
const SUB_DT: f32 = 1.0 / 120.0;
const RT_MIN: f32 = TA; // the per-substep rate tables cover the clamped temperature range, 1 K apart
const RT_N: usize = 2301; // 300..=2600 K
const P_ITERS: usize = 16; // pressure sweeps; the flame is insensitive beyond this (see examples/solver.rs)

// render
pub const UP: usize = 2;
pub const UW: usize = W * UP;
pub const UH: usize = H * UP;
const MAX_SPARKS: usize = 64;
const ABEL_R: usize = HEAD_X; // half-width of the slice about the wick axis, cells
const ABEL_NORM: f32 = 1.0 / 20.0; // divides the projected intensity by a typical chord (20 cells) so exposure values stay comparable
const ENC_N: usize = 65536; // tone-curve table entries over [0, ENC_VMAX] linear
const ENC_VMAX: f32 = 24.0;

// ---------------------------------------------------------------- spectral tables
const C2: f64 = 1.4387769e7; // nm*K
const LT_MIN: f32 = 300.0;
const LT_MAX: f32 = 7000.0;
const LT_STEP: f32 = 5.0;
const NT: usize = ((LT_MAX - LT_MIN) / LT_STEP) as usize + 1;

fn lobe(x: f64, m: f64, s1: f64, s2: f64) -> f64 {
    let t = (x - m) / if x < m { s1 } else { s2 };
    (-0.5 * t * t).exp()
}
/// Wyman, Sloan & Shirley (2013) fit of the CIE 1931 2-degree observer.
pub fn cmf(l: f64) -> [f64; 3] {
    [
        1.056 * lobe(l, 599.8, 37.9, 31.0) + 0.362 * lobe(l, 442.0, 16.0, 26.7) - 0.065 * lobe(l, 501.1, 20.4, 26.2),
        0.821 * lobe(l, 568.8, 46.9, 40.5) + 0.286 * lobe(l, 530.9, 16.3, 31.1),
        1.217 * lobe(l, 437.0, 11.8, 36.0) + 0.681 * lobe(l, 459.0, 26.0, 13.8),
    ]
}
/// Spectral radiance on a fixed arbitrary scale, lambda in nm.
pub fn planck(l: f64, t: f64) -> f64 {
    let x = C2 / (l * t);
    if x > 700.0 { 0.0 } else { 1e20 / (l.powi(5) * x.exp_m1()) }
}
fn integrate(f: impl Fn(f64) -> f64) -> [f64; 3] {
    let mut acc = [0.0; 3];
    let mut l = 360.0;
    while l <= 830.0 {
        let w = f(l) * 2.0;
        let c = cmf(l);
        for i in 0..3 { acc[i] += w * c[i]; }
        l += 2.0;
    }
    acc
}
#[inline(always)]
fn look(lut: &[f32], t: f32) -> [f32; 3] {
    let mut f = (t - LT_MIN) / LT_STEP;
    if f < 0.0 { f = 0.0 }
    if f > NT as f32 - 1.001 { f = NT as f32 - 1.001 }
    let n = f as usize;
    let a = f - n as f32;
    let k = n * 3;
    unsafe {
        [
            *lut.get_unchecked(k) + (*lut.get_unchecked(k + 3) - *lut.get_unchecked(k)) * a,
            *lut.get_unchecked(k + 1) + (*lut.get_unchecked(k + 4) - *lut.get_unchecked(k + 1)) * a,
            *lut.get_unchecked(k + 2) + (*lut.get_unchecked(k + 5) - *lut.get_unchecked(k + 2)) * a,
        ]
    }
}
/// Four Catmull-Rom taps (source indices and weights) for output sample `o` of a UPx upsample over `n` source samples.
fn cubic_taps(o: usize, n: usize) -> ([usize; 4], [f32; 4]) {
    let f = (o as f32 + 0.5) / UP as f32 - 0.5;
    let i0 = f.floor();
    let t = f - i0;
    let (t2, t3) = (t * t, t * t * t);
    let w = [(-t3 + 2.0 * t2 - t) * 0.5, (3.0 * t3 - 5.0 * t2 + 2.0) * 0.5, (-3.0 * t3 + 4.0 * t2 + t) * 0.5, (t3 - t2) * 0.5];
    let idx = |d: i32| (i0 as i32 + d).clamp(0, n as i32 - 1) as usize;
    ([idx(-1), idx(0), idx(1), idx(2)], w)
}
#[inline(always)]
fn rate_idx(t: f32) -> (usize, f32) {
    let f = (t - RT_MIN).clamp(0.0, RT_N as f32 - 1.001);
    let n = f as usize;
    (n, f - n as f32)
}
/// One channel of the per-substep rate table at temperature t, linearly interpolated.
#[inline(always)]
fn rate_at(tab: &[f32], t: f32, c: usize) -> f32 {
    let (n, a) = rate_idx(t);
    let k = n * 3 + c;
    unsafe { *tab.get_unchecked(k) + (*tab.get_unchecked(k + 3) - *tab.get_unchecked(k)) * a }
}
#[inline(always)]
fn rate3(tab: &[f32], t: f32) -> [f32; 3] {
    let (n, a) = rate_idx(t);
    let k = n * 3;
    unsafe {
        [
            *tab.get_unchecked(k) + (*tab.get_unchecked(k + 3) - *tab.get_unchecked(k)) * a,
            *tab.get_unchecked(k + 1) + (*tab.get_unchecked(k + 4) - *tab.get_unchecked(k + 1)) * a,
            *tab.get_unchecked(k + 2) + (*tab.get_unchecked(k + 5) - *tab.get_unchecked(k + 2)) * a,
        ]
    }
}
fn mul3(a: &[f64; 9], b: &[f64; 9]) -> [f64; 9] {
    let mut r = [0.0; 9];
    for i in 0..3 { for j in 0..3 { r[i * 3 + j] = a[i * 3] * b[j] + a[i * 3 + 1] * b[3 + j] + a[i * 3 + 2] * b[6 + j]; } }
    r
}

// ---------------------------------------------------------------- small helpers
#[inline(always)]
fn aces(x: f32) -> f32 {
    let x = if x < 0.0 { 0.0 } else { x };
    let y = (x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14);
    if y > 1.0 { 1.0 } else { y }
}
const INF: [[f32; 3]; 8] = [
    [0.0, 0.0, 4.0], [40.0, 11.0, 84.0], [101.0, 21.0, 110.0], [159.0, 42.0, 99.0],
    [212.0, 72.0, 66.0], [245.0, 125.0, 21.0], [250.0, 193.0, 39.0], [252.0, 255.0, 164.0],
];
fn inferno(t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    let x = t * 7.0;
    let i = (x as usize).min(6);
    let a = x - i as f32;
    let (p, q) = (INF[i], INF[i + 1]);
    [(p[0] + (q[0] - p[0]) * a) / 255.0, (p[1] + (q[1] - p[1]) * a) / 255.0, (p[2] + (q[2] - p[2]) * a) / 255.0]
}

#[derive(Clone)]
struct Rng(u32);
impl Rng {
    fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13; x ^= x >> 17; x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / 16777216.0
    }
    fn gauss(&mut self) -> f32 { (self.next() + self.next() + self.next() + self.next() - 2.0) * 1.732 }
}

#[derive(Clone, Copy)]
struct Spark { x: f32, y: f32, px: f32, py: f32, vx: f32, vy: f32, t: f32 }

#[derive(Clone, Copy, PartialEq)]
enum Phase { Ready = 0, Lit = 1, Out = 2 }

// ---------------------------------------------------------------- field kernels (free fns: no borrow fights)
fn edges(d: &mut [f32], amb: Option<f32>) {
    for j in 0..H {
        d[j * W] = amb.unwrap_or(d[1 + j * W]);
        d[W - 1 + j * W] = amb.unwrap_or(d[W - 2 + j * W]);
    }
    for i in 0..W {
        d[i] = d[i + W];
        d[i + (H - 1) * W] = amb.unwrap_or(d[i + (H - 2) * W]);
    }
}

/// Semi-Lagrangian advection. All six fields ride the same velocity, so trace each cell back once and sample every field there.
fn advect_all(dst: [&mut [f32]; 6], src: [&[f32]; 6], amb: [Option<f32>; 6], dt: f32, uu: &[f32], vv: &[f32]) {
    let [d0, d1, d2, d3, d4, d5] = dst;
    let (xmax, ymax) = (W as f32 - 1.5, H as f32 - 1.5);
    for j in 1..H - 1 {
        for i in 1..W - 1 {
            let k = i + j * W;
            unsafe {
                let x = (i as f32 - dt * uu.get_unchecked(k)).clamp(0.5, xmax);
                let y = (j as f32 - dt * vv.get_unchecked(k)).clamp(0.5, ymax);
                let (i0, j0) = (x as usize, y as usize);
                let (a, b) = (x - i0 as f32, y - j0 as f32);
                let q = i0 + j0 * W;
                let (w00, w10, w01, w11) = ((1.0 - a) * (1.0 - b), a * (1.0 - b), (1.0 - a) * b, a * b);
                macro_rules! s { ($d:expr, $s:expr) => {
                    *$d.get_unchecked_mut(k) = w00 * $s.get_unchecked(q) + w10 * $s.get_unchecked(q + 1) + w01 * $s.get_unchecked(q + W) + w11 * $s.get_unchecked(q + W + 1);
                } }
                s!(d0, src[0]); s!(d1, src[1]); s!(d2, src[2]); s!(d3, src[3]); s!(d4, src[4]); s!(d5, src[5]);
            }
        }
    }
    edges(d0, amb[0]); edges(d1, amb[1]); edges(d2, amb[2]); edges(d3, amb[3]); edges(d4, amb[4]); edges(d5, amb[5]);
}


// ---------------------------------------------------------------- diffusion with temperature-dependent coefficients
/// dphi/dt = div(D grad phi) by locally one-dimensional implicit sweeps: a backward-Euler step along x, then along y,
/// each row (or column) a tridiagonal system solved by the Thomas algorithm. Unconditionally stable and conservative.
/// `gx[k]` and `gy[k]` hold dt/dx^2 times the diffusivity factor of the face to the right of / below cell k (harmonic
/// mean of the two cells, so a wax face carries no flux); `d0` scales them for the field: alpha, alpha/Le or nu.
/// `e` is an N-sized scratch.
fn diffuse_adi(x: &mut [f32], gx: &[f32], gy: &[f32], d0: f32, e: &mut [f32]) {
    const _: () = assert!(H % 4 == 0);
    // x: every row is an independent chain, so run four at a time and let the divisions overlap.
    // e[..4W] holds the four rows' elimination factors, e[4W..8W] their modified right-hand sides.
    let (ef, df) = e[..8 * W].split_at_mut(4 * W);
    for j in (0..H).step_by(4) {
        let (mut ep, mut dp) = ([0f32; 4], [0f32; 4]);
        for i in 0..W {
            for r in 0..4 {
                let k = (j + r) * W + i;
                let a = if i > 0 { d0 * gx[k - 1] } else { 0.0 };
                let c = d0 * gx[k]; // gx is zero on the last column
                let inv = 1.0 / (1.0 + a + c - a * ep[r]);
                ep[r] = c * inv;
                dp[r] = (x[k] + a * dp[r]) * inv;
                ef[r * W + i] = ep[r];
                df[r * W + i] = dp[r];
            }
        }
        for r in 0..4 {
            let (es, ds) = (&ef[r * W..(r + 1) * W], &df[r * W..(r + 1) * W]);
            let xr = &mut x[(j + r) * W..(j + r + 1) * W];
            let mut xn = ds[W - 1];
            xr[W - 1] = xn;
            for i in (0..W - 1).rev() { xn = ds[i] + es[i] * xn; xr[i] = xn; }
        }
    }
    // y: all columns at once, sweeping the rows down (elimination) then up (substitution). x holds d', e the factors.
    for j in 0..H {
        let (above, here) = x.split_at_mut(j * W);
        let here = &mut here[..W];
        let (e_above, e_here) = e.split_at_mut(j * W);
        let e_here = &mut e_here[..W];
        let gc = &gy[j * W..(j + 1) * W]; // zero on the last row
        if j == 0 {
            for i in 0..W { let c = d0 * gc[i]; let inv = 1.0 / (1.0 + c); e_here[i] = c * inv; here[i] *= inv; }
        } else {
            let (ga, ea, da) = (&gy[(j - 1) * W..j * W], &e_above[(j - 1) * W..j * W], &above[(j - 1) * W..j * W]);
            for i in 0..W {
                let (a, c) = (d0 * ga[i], d0 * gc[i]);
                let inv = 1.0 / (1.0 + a + c - a * ea[i]);
                e_here[i] = c * inv;
                here[i] = (here[i] + a * da[i]) * inv;
            }
        }
    }
    for j in (0..H - 1).rev() {
        let (here, below) = x.split_at_mut((j + 1) * W);
        let (here, below, ej) = (&mut here[j * W..], &below[..W], &e[j * W..(j + 1) * W]);
        for i in 0..W { here[i] += ej[i] * below[i]; }
    }
}

// ---------------------------------------------------------------- the simulation
// ---------------------------------------------------------------- the simulation
#[derive(Clone)]
pub struct Sim {
    u: Vec<f32>, v: Vec<f32>, u0: Vec<f32>, v0: Vec<f32>,
    t: Vec<f32>, t0: Vec<f32>, f: Vec<f32>, f0: Vec<f32>,
    o: Vec<f32>, o0: Vec<f32>, s: Vec<f32>, s0: Vec<f32>,
    r: Vec<f32>, p: Vec<f32>, dv: Vec<f32>, cu: Vec<f32>, solid: Vec<u8>,
    // room
    o2: f32, g: f32, swirl: f32, draft_k: f32,
    p_iters: usize, // pressure sweeps, adjustable at runtime for experiments
    alpha0: f32, nu0: f32, // thermal diffusivity and kinematic viscosity at 300 K, mm^2/s
    fuel: f32, // see FUEL
    curl: f32, supply_sigma: f32, supply: f32, // wick curl in cells, supply noise amplitude, and its current state
    draft_sigma: f32, draft_tau: f32, draft_coupling: f32, // the room-air model, see the DRAFT_* constants
    soot_scale: f32, krad: f32, // tuning multipliers on the empirical soot-formation rate and the radiative-loss coefficient
    q: f32, // heat release per unit fuel, K (see Q)
    gtab: Vec<f32>, gx: Vec<f32>, gy: Vec<f32>, // (T/300)^D_EXP per K; face coefficients for the current substep
    // match state
    phase: Phase, time: f32, burned: f32, h: f32, charge: f32, ignite: f32, ember: f32,
    out_timer: f32, gust: f32, gust_v: f32, draft: f32, qrel: f32, qrad: f32, since: f32,
    sparks: Vec<Spark>, rng: Rng,
    // light
    soot: Vec<f32>, grey: Vec<f32>, chemi: [f32; 3], m: [f32; 9],
    rate: Vec<f32>, // per K: burn fraction per substep, soot formation per unit fuel, soot oxidation coefficient
    abel: Vec<f32>, // line-of-sight weights, two per (ray m, annulus n): own side and other side of the slice (see new())
    expo: f32, chemi_gain: f32, view: u32,
    lin: Vec<f32>, pix: Vec<u8>, enc: Vec<u8>, taps: Vec<([u32; 4], [f32; 4])>, rowbuf: Vec<f32>,
    pub stats: [f32; 24], spark_buf: [f32; MAX_SPARKS * 5], scratch: [f32; 4],
    pub sim_ms: f32,
}

impl Sim {
    pub fn new() -> Sim {
        // spectral lookup tables (f64 while integrating)
        let mut soot = vec![0f64; NT * 3];
        let mut grey = vec![0f64; NT * 3];
        for n in 0..NT {
            let t = LT_MIN as f64 + n as f64 * LT_STEP as f64;
            let s = integrate(|l| planck(l, t) / l); // soot: emissivity ~ 1/lambda
            let b = integrate(|l| planck(l, t)); // grey body: char, sparks
            soot[n * 3..n * 3 + 3].copy_from_slice(&s);
            grey[n * 3..n * 3 + 3].copy_from_slice(&b);
        }
        let ri = ((1800.0 - LT_MIN) / LT_STEP) as usize * 3 + 1;
        let (sr, gr) = (soot[ri], grey[ri]);
        let soot: Vec<f32> = soot.iter().map(|v| (v / sr) as f32).collect();
        let grey: Vec<f32> = grey.iter().map(|v| (v / gr) as f32).collect();
        // blue-zone chemiluminescence: CH* 431 nm, C2* Swan bands, CO2* continuum
        let lines = [(431.4, 3.0, 1.0), (516.5, 3.0, 0.55), (473.7, 3.0, 0.25), (563.5, 3.0, 0.2), (455.0, 60.0, 0.9)];
        let c = integrate(|l| lines.iter().map(|&(m, w, a): &(f64, f64, f64)| { let t = (l - m) / w; a * (-0.5 * t * t).exp() / w }).sum());
        let chemi = [(c[0] / c[1]) as f32, 1.0, (c[2] / c[1]) as f32];
        // the three Arrhenius rates, integrated over one substep, as functions of temperature (the hot loop then
        // does one table lookup instead of exp calls). Same formulas as before, evaluated once in f64.
        let rate = Self::rate_table(1.0);
        // Abel projection of the slice: the camera sees an optically thin, roughly axisymmetric flame, so the intensity at
        // offset x from the axis is the integral of the slice emission along the chord through all annuli at r >= x.
        // Cell n on one side of the axis is the annulus [n, n+1]; the ray through cell m is at x = m + 0.5.
        // The flame sways, so the two halves of the slice differ. Along the chord the azimuth runs from the ray's own side
        // (cos = x/r) round to the far side, so the emission is taken as the two halves mixed by (1 +- x/r)/2: central
        // rays see the mean of both halves, limb rays their own. abel[(m * ABEL_R + n) * 2] is the own-side weight.
        let mut abel = vec![0f32; ABEL_R * ABEL_R * 2];
        for m in 0..ABEL_R {
            let x = m as f64 + 0.5;
            for n in m..ABEL_R {
                let (a, b) = ((n as f64).max(x), n as f64 + 1.0);
                let chord = 2.0 * ((b * b - x * x).sqrt() - (a * a - x * x).sqrt()) * ABEL_NORM as f64;
                let own = 0.5 * (1.0 + x / (n as f64 + 0.5));
                abel[(m * ABEL_R + n) * 2] = (chord * own) as f32;
                abel[(m * ABEL_R + n) * 2 + 1] = (chord * (1.0 - own)) as f32;
            }
        }
        // linear light -> ACES tone curve -> sRGB 8-bit
        let mut enc = vec![0u8; ENC_N];
        for (i, e) in enc.iter_mut().enumerate() {
            let v = i as f32 / (ENC_N - 1) as f32 * ENC_VMAX;
            let c = aces(v) as f64;
            let srgb = if c <= 0.0031308 { 12.92 * c } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
            *e = (srgb * 255.0).round().clamp(0.0, 255.0) as u8;
        }
        let z = || vec![0f32; N];
        let mut sim = Sim {
            u: z(), v: z(), u0: z(), v0: z(), t: z(), t0: z(), f: z(), f0: z(), o: z(), o0: z(), s: z(), s0: z(),
            r: z(), p: z(), dv: z(), cu: z(), solid: vec![0; N],
            o2: 0.21, g: 1.0, swirl: 1.0, draft_k: 1.0,
            p_iters: P_ITERS, alpha0: ALPHA0, nu0: PR * ALPHA0, fuel: FUEL, curl: WICK_CURL, supply_sigma: SUPPLY_SIGMA, supply: 0.0, draft_sigma: DRAFT_SIGMA, draft_tau: DRAFT_TAU, draft_coupling: DRAFT_COUPLING, soot_scale: 1.0, krad: KRAD, q: Q,
            gtab: (0..RT_N).map(|n| ((RT_MIN + n as f32) / TA).powf(D_EXP)).collect(), gx: z(), gy: z(),
            phase: Phase::Ready, time: 0.0, burned: 0.0, h: 0.0, charge: 0.0, ignite: 0.0, ember: TA,
            out_timer: 0.0, gust: 0.0, gust_v: 0.0, draft: 0.0, qrel: 0.0, qrad: 0.0, since: 0.0,
            sparks: Vec::with_capacity(MAX_SPARKS), rng: Rng(0x9e3779b9),
            soot, grey, chemi, m: [0.0; 9], rate, abel, expo: 4.2, chemi_gain: 0.00022, view: 0,
            lin: vec![0.0; N * 3], pix: vec![0; UW * UH * 4], enc,
            taps: (0..UW).map(|x| { let (q, w) = cubic_taps(x, W); ([q[0] as u32 * 3, q[1] as u32 * 3, q[2] as u32 * 3, q[3] as u32 * 3], w) }).collect(),
            rowbuf: vec![0.0; W * 3],
            stats: [0.0; 24], spark_buf: [0.0; MAX_SPARKS * 5], scratch: [0.0; 4], sim_ms: 0.0,
        };
        sim.set_wb(2800.0);
        sim.reset();
        sim
    }

    /// The three Arrhenius rates integrated over one substep, per kelvin; `a_scale` multiplies the burn pre-exponential.
    fn rate_table(a_scale: f64) -> Vec<f32> {
        let mut rate = vec![0f32; RT_N * 3];
        for n in 0..RT_N {
            let t = RT_MIN as f64 + n as f64;
            let (dt, a, ea, sf, esf, so, eso) = (SUB_DT as f64, A as f64 * a_scale, EA as f64, SF as f64, ESF as f64, SO as f64, ESO as f64);
            rate[n * 3] = (1.0 - (-a * (-ea / t).exp() * dt).exp()) as f32; // fraction of the limiting reactant burned
            rate[n * 3 + 1] = (dt * sf * (-esf / t).exp()) as f32; // soot formed per unit fuel
            rate[n * 3 + 2] = (dt * so * (-eso / t).exp()) as f32; // soot oxidation coefficient, times (O2 + OH proxy)
        }
        rate
    }

    /// Camera white balance: Bradford adaptation from a blackbody white to D65, then XYZ -> linear sRGB.
    pub fn set_wb(&mut self, wb_k: f32) {
        let w = look(&self.grey, wb_k);
        let s = [(w[0] / w[1]) as f64, 1.0, (w[2] / w[1]) as f64];
        let d = [0.95047, 1.0, 1.08883];
        let br = [0.8951, 0.2664, -0.1614, -0.7502, 1.7135, 0.0367, 0.0389, -0.0685, 1.0296];
        let bri = [0.9869929, -0.1470543, 0.1599627, 0.4323053, 0.5183603, 0.0492912, -0.0085287, 0.0400428, 0.9684867];
        let to_srgb = [3.2406, -1.5372, -0.4986, -0.9689, 1.8758, 0.0415, 0.0557, -0.2040, 1.0570];
        let dot = |row: usize, v: &[f64; 3]| br[row * 3] * v[0] + br[row * 3 + 1] * v[1] + br[row * 3 + 2] * v[2];
        let dm = [dot(0, &d) / dot(0, &s), 0.0, 0.0, 0.0, dot(1, &d) / dot(1, &s), 0.0, 0.0, 0.0, dot(2, &d) / dot(2, &s)];
        let m = mul3(&to_srgb, &mul3(&bri, &mul3(&dm, &br)));
        for i in 0..9 { self.m[i] = m[i] as f32; }
    }

    pub fn reset(&mut self) {
        self.u.fill(0.0); self.v.fill(0.0); self.t.fill(TA); self.f.fill(0.0);
        self.s.fill(0.0); self.r.fill(0.0); self.p.fill(0.0);
        self.o.fill(self.o2 / 0.21);
        self.phase = Phase::Ready;
        self.time = 0.0; self.burned = 0.0; self.h = 0.0; self.charge = 0.0; self.ignite = 0.0;
        self.ember = TA; self.out_timer = 0.0; self.gust = 0.0; self.gust_v = 0.0; self.draft = 0.0;
        self.qrel = 0.0; self.qrad = 0.0; self.since = 0.0; self.sparks.clear(); self.supply = 0.0;
        self.build_solid();
    }

    fn head_y(&self) -> f32 { HEAD0 + self.burned }

    /// The wax column is solid; the wick is thin enough to leave to the hot-surface model below.
    fn build_solid(&mut self) {
        self.solid.fill(0);
        for j in WAX_TOP..H { for i in HEAD_X - WAX_HALF..HEAD_X + WAX_HALF { self.solid[i + j * W] = 1; } }
    }

    /// Wax vapour leaving the wick, driven by the heat feedback from the flame.
    fn src_strength(&self) -> f32 {
        match self.phase {
            Phase::Lit => (0.2 + 0.8 * self.h + self.charge * 0.6).min(1.0) * self.fuel * (1.0 + self.supply),
            // just blown out: the hot wick keeps vaporising wax for a few seconds, which shows as smoke
            Phase::Out => 0.25 * (-self.since / 2.5).exp(),
            Phase::Ready => 0.0,
        }
    }

    /// Hottest gas touching the head: is the flame still attached?
    fn near_t(&self) -> f32 {
        let hy = self.head_y() as i32;
        let mut m = TA;
        for j in -24..=4i32 {
            for i in -16..=16i32 {
                let k = (HEAD_X as i32 + i + (hy + j) * W as i32) as usize;
                if self.t[k] > m { m = self.t[k]; }
            }
        }
        m
    }

    pub fn project(&mut self) {
        let iters = self.p_iters;
        let Sim { u, v, p, dv, r, t, solid, .. } = self;
        for j in 1..H - 1 {
            for i in 1..W - 1 {
                // thermal expansion: burning gas swells, so the flow is not divergence-free where heat is released
                let k = i + j * W;
                dv[k] = 0.5 * (u[k + 1] - u[k - 1] + v[k + W] - v[k - W]) - EXPAND * Q * r[k] / t[k];
            }
        }
        p.fill(0.0);
        for it in 0..iters {
            for pass in 0..2 {
                for j in 1..H - 1 {
                    let mut i = 1 + (j + it + pass) % 2;
                    while i < W - 1 {
                        let k = i + j * W;
                        unsafe {
                            *p.get_unchecked_mut(k) = (p.get_unchecked(k - 1) + p.get_unchecked(k + 1) + p.get_unchecked(k - W) + p.get_unchecked(k + W) - dv.get_unchecked(k)) * 0.25;
                        }
                        i += 2;
                    }
                }
            }
        }
        for j in 1..H - 1 {
            for i in 1..W - 1 {
                let k = i + j * W;
                u[k] -= 0.5 * (p[k + 1] - p[k - 1]);
                v[k] -= 0.5 * (p[k + W] - p[k - W]);
            }
        }
        edges(u, None); edges(v, None);
        for i in 0..W { if v[i] > 0.0 { v[i] = 0.0; } } // top is an outlet: no backflow
        for k in 0..N { if solid[k] != 0 { u[k] = 0.0; v[k] = 0.0; } }
    }

    fn substep(&mut self, dt: f32) -> f32 {
        self.forces(dt);
        self.wick(dt);
        self.transport(dt);
        self.entrain(dt);
        self.chemistry(dt)
    }

    /// Buoyancy, vorticity confinement, the room draft with its eddies, and the gust.
    fn forces(&mut self, dt: f32) {
        let hy = self.head_y();
        let gb = G * self.g;
        for k in 0..N { self.v[k] -= dt * gb * (self.t[k] - TA) / self.t[k]; }
        {
            let Sim { u, v, cu, .. } = self;
            for j in 1..H - 1 {
                for i in 1..W - 1 {
                    let k = i + j * W;
                    cu[k] = 0.5 * (v[k + 1] - v[k - 1] - u[k + W] + u[k - W]);
                }
            }
            let eps = 24.0 * self.swirl;
            if eps > 0.0 {
                for j in 2..H - 2 {
                    for i in 2..W - 2 {
                        let k = i + j * W;
                        let nx = 0.5 * (cu[k + 1].abs() - cu[k - 1].abs());
                        let ny = 0.5 * (cu[k + W].abs() - cu[k - W].abs());
                        let l = (nx * nx + ny * ny).sqrt() + 1e-5;
                        let w = cu[k];
                        u[k] += dt * eps * (ny / l) * w;
                        v[k] -= dt * eps * (nx / l) * w;
                    }
                }
            }
        }
        // room draft: slowly wandering air current (Ornstein-Uhlenbeck), gently damped, plus small eddies
        let (tau, cpl) = (self.draft_tau, self.draft_coupling);
        self.draft += (-self.draft / tau) * dt + (2.0 * dt / tau).sqrt() * self.rng.gauss() * self.draft_k * self.draft_sigma / DX;
        let dr = self.draft;
        for k in 0..N { self.u[k] += (dr - self.u[k]) * cpl * dt; }
        if self.rng.next() < dt * 12.0 * self.draft_k {
            let ex = HEAD_X as f32 + (self.rng.next() - 0.5) * 60.0;
            let ey = hy + 8.0 - self.rng.next() * 50.0;
            let sg = (self.rng.next() - 0.5) * 1000.0 * self.draft_k;
            for j in -8..=8i32 {
                for i in -8..=8i32 {
                    let (xx, yy) = ((ex + i as f32) as i32, (ey + j as f32) as i32);
                    if xx < 1 || yy < 1 || xx > W as i32 - 2 || yy > H as i32 - 2 { continue; }
                    let w = (-((i * i + j * j) as f32) / 24.0).exp();
                    let k = xx as usize + yy as usize * W;
                    self.u[k] += -(j as f32) * w * sg * dt * 10.0;
                    self.v[k] += i as f32 * w * sg * dt * 10.0;
                }
            }
        }
        if self.gust > 0.0 {
            let (y0, y1) = (hy - 140.0, hy + 12.0);
            let a = (dt * 25.0).min(1.0);
            let mut j = (y0 as i32).max(1) as usize;
            while j < H - 1 && (j as f32) < y1 {
                let prof = (PI * (j as f32 - y0) / (y1 - y0)).sin();
                for i in 1..W - 1 { let k = i + j * W; self.u[k] += (self.gust_v * prof - self.u[k]) * a * prof; }
                j += 1;
            }
        }
    }

    /// The wick: wax vapour leaves it, and the glowing surface heats the gas touching it.
    fn wick(&mut self, dt: f32) {
        let hy = self.head_y();
        let pyro = self.src_strength();
        let ts = self.ember.max(if self.phase == Phase::Out { TA + 200.0 * (-self.since / 2.5).exp() } else { TA });
        // the curled wick: its glowing tip, and most of the vapour, sit to one side of the axis
        let (cx_heat, cx_fuel) = (0.5 * self.curl, 0.7 * self.curl);
        for j in -16..=10i32 {
            for i in -14..=22i32 {
                let (dh, df) = (i as f32 - cx_heat, i as f32 - cx_fuel);
                let e = dh * dh / 196.0 + (j * j) as f32 / 200.0;
                if e > 1.0 { continue; }
                let k = (HEAD_X as i32 + i + ((hy + j as f32) as i32) * W as i32) as usize;
                if self.solid[k] != 0 { continue; }
                if self.t[k] < ts { self.t[k] += (ts - self.t[k]) * 0.5 * ((1.0 - e) * 2.5).min(1.0); }
                let ei = df * df / 120.0 + (j * j) as f32 / 96.0;
                let wi = ((1.0 - ei) * 2.0).clamp(0.0, 1.0);
                if wi > 0.0 {
                    if pyro > 0.0 {
                        self.f[k] = self.f[k].max(pyro * wi);
                        self.o[k] *= 1.0 - 0.9 * pyro.min(1.0) * wi;
                        self.v[k] = self.v[k].min(-120.0 * pyro * wi);
                        self.u[k] += df * 8.0 * pyro * wi * dt * 60.0;
                    }
                    if self.ignite > 0.0 { self.t[k] = self.t[k].max(2300.0); }
                    if self.charge > 0.0 { self.t[k] = (self.t[k] + 4000.0 * self.charge * dt).min(2600.0); }
                }
            }
        }
    }

    /// Make the velocity divergence-free, carry everything along it, then diffuse.
    fn transport(&mut self, dt: f32) {
        let oa = self.o2 / 0.21;
        self.project();
        std::mem::swap(&mut self.u, &mut self.u0);
        std::mem::swap(&mut self.v, &mut self.v0);
        std::mem::swap(&mut self.t, &mut self.t0);
        std::mem::swap(&mut self.f, &mut self.f0);
        std::mem::swap(&mut self.o, &mut self.o0);
        std::mem::swap(&mut self.s, &mut self.s0);
        {
            let Sim { u, v, u0, v0, t, t0, f, f0, o, o0, s, s0, dv, cu, gtab, gx, gy, solid, alpha0, nu0, .. } = self;
            advect_all([&mut t[..], &mut f[..], &mut o[..], &mut s[..], &mut u[..], &mut v[..]],
                       [&t0[..], &f0[..], &o0[..], &s0[..], &u0[..], &v0[..]],
                       [Some(TA), Some(0.0), Some(oa), Some(0.0), None, None], dt, u0, v0);
            // diffusivity factor per cell, zero in the wax (it insulates), then the face coefficients, harmonic mean
            for k in 0..N { cu[k] = if solid[k] != 0 { 0.0 } else { gtab[rate_idx(t[k]).0] }; }
            let lam = dt / (DX * DX);
            for k in 0..N {
                let g = cu[k];
                let gr = if (k + 1) % W != 0 { cu[k + 1] } else { 0.0 };
                let gb = if k + W < N { cu[k + W] } else { 0.0 };
                gx[k] = lam * 2.0 * g * gr / (g + gr + 1e-6);
                gy[k] = lam * 2.0 * g * gb / (g + gb + 1e-6);
            }
            let (alpha, nu) = (*alpha0, *nu0);
            diffuse_adi(t, gx, gy, alpha, dv);
            diffuse_adi(o, gx, gy, alpha / LE_O, dv);
            diffuse_adi(f, gx, gy, alpha / LE_F, dv);
            diffuse_adi(u, gx, gy, nu, dv);
            diffuse_adi(v, gx, gy, nu, dv);
            // top boundary is an outlet: zero-gradient
            for i in 0..W { t[i] = t[i + W]; f[i] = f[i + W]; o[i] = o[i + W]; s[i] = s[i + W]; }
        }
        for k in 0..N { if self.solid[k] != 0 { self.u[k] = 0.0; self.v[k] = 0.0; } }
    }

    /// Out-of-plane entrainment: a real flame is round, so air also arrives from the third dimension.
    fn entrain(&mut self, dt: f32) {
        let oa = self.o2 / 0.21;
        {
            let Sim { u, v, t, f, s, o, .. } = self;
            for k in 0..N {
                if t[k] < 320.0 && f[k] < 1e-3 { continue; }
                let x = dt * ENT * (u[k] * u[k] + v[k] * v[k]).sqrt() / ENT_R;
                let a = if x < 1.0 { x * (1.0 - 0.5 * x) } else { 1.0 - (-x).exp() }; // 1 - e^-x to third order; the exp only for gusts
                t[k] += (TA - t[k]) * a;
                f[k] -= f[k] * a;
                s[k] -= s[k] * a;
                o[k] += (oa - o[k]) * a;
            }
        }
    }

    /// One-step chemistry, soot formation and oxidation, optically thin radiation. Returns the heat-release rate.
    fn chemistry(&mut self, dt: f32) -> f32 {
        let oa = self.o2 / 0.21;
        let mut rsum = 0.0f32;
        let mut qrad = 0.0f32;
        {
            let Sim { t: tt, f: ff, o: oo, s: ss, r: rr, solid, rate, soot_scale, krad, q, .. } = self;
            let (ssc, kr, q) = (*soot_scale, *krad, *q);
            for k in 0..N {
                let (mut t, mut f, mut o, mut s) = (tt[k], ff[k], oo[k], ss[k]);
                let mut r = 0.0;
                if t > 600.0 && f > 1e-5 && o > 1e-5 {
                    let kb = rate_at(rate, t, 0); // fraction of the limiting reactant that burns this substep
                    let lim = f.min(o / STO);
                    r = lim * kb;
                    f -= r; o -= STO * r; t += q * r;
                }
                if t > 900.0 {
                    let [_, ks, ko] = rate3(rate, t);
                    if t > 1100.0 && f > 0.005 { s += ks * ssc * f; }
                    // O2 + OH (proxy) attack soot
                    if s > 1e-6 { s -= s * (1.0 - (-ko * (o + OHP)).exp()); }
                }
                let t4 = t * t * t * t;
                let ql = dt * kr * (s + 0.04) * (t4 - 8.1e9);
                t -= ql; qrad += ql;
                // flush tiny values to zero: denormal floats are very slow on most CPUs
                let rn = rr[k] * 0.5 + 0.5 * r / dt;
                tt[k] = t.clamp(TA, 2600.0);
                ff[k] = if f < 1e-9 { 0.0 } else { f };
                oo[k] = o;
                ss[k] = if s < 1e-9 { 0.0 } else { s.min(4.0) };
                rr[k] = if rn < 1e-9 { 0.0 } else { rn };
                rsum += r;
            }
            for k in 0..N { if solid[k] != 0 { ff[k] = 0.0; ss[k] = 0.0; tt[k] = TA; oo[k] = oa; } }
        }
        let (rsum, qrad) = (rsum * CELL_AREA, qrad * CELL_AREA); // per unit area, so thresholds and ratios are grid-independent
        self.qrel = self.qrel * 0.98 + 0.02 * self.q * rsum;
        self.qrad = self.qrad * 0.98 + 0.02 * qrad;
        rsum / dt
    }

    pub fn step(&mut self, frame_dt: f32) {
        let n = ((frame_dt / SUB_DT).round() as i32).clamp(1, 3);
        for _ in 0..n {
            let heat = self.substep(SUB_DT);
            let dt = SUB_DT;
            self.time += dt; self.since += dt;
            self.supply += (-self.supply / SUPPLY_TAU) * dt + (2.0 * dt / SUPPLY_TAU).sqrt() * self.rng.gauss() * self.supply_sigma;
            self.supply = self.supply.clamp(-0.5, 0.5);
            if self.ignite > 0.0 { self.ignite -= dt; }
            self.charge *= (-dt / 0.45).exp();
            if self.charge < 0.01 { self.charge = 0.0; }
            if self.gust > 0.0 {
                self.gust -= dt;
                // forced convection cools the wick
                self.ember -= dt * 0.4 * self.gust_v.abs() * DX / 1000.0 * (self.ember - TA);
            }
            match self.phase {
                Phase::Lit => {
                    let nt = self.near_t();
                    let target = ((nt - 900.0) / 600.0).clamp(0.0, 1.0);
                    self.h += (target - self.h) * (1.0 - (-dt / 0.35).exp());
                    let et = (TA + 0.6 * (nt - TA)).min(1150.0) + 300.0 * self.charge;
                    self.ember += (et - self.ember) * (1.0 - (-dt / 0.6).exp());
                    self.out_timer = if heat < 0.02 { self.out_timer + dt } else { 0.0 };
                    if self.since > 0.4 && self.out_timer > 0.25 { self.phase = Phase::Out; self.since = 0.0; }
                }
                Phase::Out => { self.ember = TA + (self.ember - TA) * (-dt / 2.2).exp(); self.h = 0.0; }
                Phase::Ready => {}
            }
        }
        self.step_sparks(n as f32 * SUB_DT);
    }

    fn step_sparks(&mut self, dt: f32) {
        let g = G * 0.5 * self.g; // half g: drag from the rising gas
        let damp = (-dt * 3.0).exp();
        for p in self.sparks.iter_mut() {
            p.px = p.x; p.py = p.y;
            p.vy += g * dt;
            p.vx *= damp; p.vy *= damp;
            p.x += p.vx * dt; p.y += p.vy * dt;
            // radiative (T^4) + convective cooling of a ~50 micron particle
            let t = p.t;
            p.t -= dt * (2.2e-10 * (t * t * t * t - 8.1e9) + 1.6 * (t - TA));
        }
        self.sparks.retain(|p| p.t > 750.0 && p.y < H as f32 + 20.0);
    }

    pub fn strike(&mut self, with_sparks: bool) -> bool {
        if self.phase != Phase::Ready { return false; }
        self.phase = Phase::Lit; self.since = 0.0; self.ignite = 0.12; self.charge = 1.0; self.h = 0.5; self.ember = 1300.0;
        if with_sparks {
            for _ in 0..28 {
                let a = -PI / 2.0 + (self.rng.next() - 0.5) * 2.4;
                let s = 300.0 + self.rng.next() * 700.0;
                let x = HEAD_X as f32 + (self.rng.next() - 0.5) * 6.0;
                let y = self.head_y() - 2.0;
                let t = 2100.0 + self.rng.next() * 600.0;
                self.sparks.push(Spark { x, y, px: x, py: y, vx: a.cos() * s, vy: a.sin() * s, t });
            }
        }
        true
    }

    pub fn blow(&mut self, speed_ms: f32, dur: f32) { self.gust = dur; self.gust_v = speed_ms * 1000.0 / DX; }

    pub fn poke(&mut self, x: f32, y: f32, vx: f32, vy: f32) {
        let r = 14i32;
        let (x0, y0) = (x as i32, y as i32);
        for j in -r..=r {
            for i in -r..=r {
                let (xx, yy) = (x0 + i, y0 + j);
                if xx < 1 || yy < 1 || xx > W as i32 - 2 || yy > H as i32 - 2 { continue; }
                let w = (-((i * i + j * j) as f32) / (r * r) as f32 / 0.4).exp();
                let k = xx as usize + yy as usize * W;
                self.u[k] += (vx - self.u[k]) * w * 0.6;
                self.v[k] += (vy - self.v[k]) * w * 0.6;
            }
        }
    }

    /// Radiance of every cell -> RGBA pixels at 2x, plus summary stats for the JS side.
    pub fn render(&mut self) {
        let Sim { t: tt, s: ss, r: rr, f: fu, lin, pix, enc, soot, m, .. } = self;
        let (expo, cg, view, ch) = (self.expo, self.chemi_gain, self.view, self.chemi);
        let smoke_f = self.phase == Phase::Out;
        let (mut sx, mut sy, mut sz, mut wt, mut ws, mut cx, mut cy) = (0f32, 0f32, 0f32, 0f32, 0f32, 0f32, 0f32);
        // rows with no light at all skip the upsample below; only the camera view has exact zeros to skip
        let mut row_lit = [true; H];
        for j in 0..H {
            let mut rowy = 0f32;
            let mut lit = false;
            for i in 0..W {
                let k = i + j * W;
                let t = tt[k];
                let (mut x, mut y, mut z) = (0.0, 0.0, 0.0);
                let sk = ss[k];
                if view == 0 && sk < 1e-5 && rr[k] < 1e-6 && !(smoke_f && fu[k] > 1e-4) {
                    let o = k * 3;
                    lin[o] = 0.0; lin[o + 1] = 0.0; lin[o + 2] = 0.0;
                    continue;
                }
                lit = true;
                if sk > 1e-5 && t > 700.0 {
                    let c = look(soot, t);
                    x = sk * c[0]; y = sk * c[1]; z = sk * c[2];
                    wt += y * t; ws += y;
                }
                let rc = rr[k] * cg;
                x += rc * ch[0]; y += rc * ch[1]; z += rc * ch[2];
                sx += x; sy += y; sz += z; rowy += y; cx += y * i as f32;
                let o = k * 3;
                match view {
                    0 => {
                        let mut r = (m[0] * x + m[1] * y + m[2] * z) * expo;
                        let mut g = (m[3] * x + m[4] * y + m[5] * z) * expo;
                        let mut b = (m[6] * x + m[7] * y + m[8] * z) * expo;
                        if t < 900.0 { // smoke scatters a little room light
                            let sm = ((if smoke_f { fu[k] * 4.0 } else { 0.0 }) + sk * 1.2).min(1.0) * 0.05;
                            r += sm; g += sm * 1.02; b += sm * 1.08;
                        }
                        lin[o] = r; lin[o + 1] = g; lin[o + 2] = b;
                    }
                    1 => { let c = inferno((t - 300.0) / 1900.0); lin[o] = c[0]; lin[o + 1] = c[1]; lin[o + 2] = c[2]; }
                    2 => { let v = (sk / 0.5).sqrt().min(1.0); lin[o] = v; lin[o + 1] = v; lin[o + 2] = v; }
                    _ => { let v = (rr[k] / 12.0).sqrt().min(1.0); lin[o] = 0.64 * v; lin[o + 1] = 0.59 * v; lin[o + 2] = v; }
                }
            }
            cy += rowy * j as f32;
            if view == 0 { row_lit[j] = lit; }
        }
        // camera view: replace each row of the slice by its line-of-sight projection, one side of the axis at a time
        if view == 0 {
            let Sim { abel, rowbuf, .. } = self;
            for j in 0..H {
                if !row_lit[j] { continue; }
                let row = &mut lin[j * W * 3..(j + 1) * W * 3];
                // annulus n is column HEAD_X + n on the right and HEAD_X - 1 - n on the left: the axis is the edge between them
                let (cr, cl) = (|n: usize| (HEAD_X + n) * 3, |n: usize| (HEAD_X - 1 - n) * 3);
                let lit = |o: usize| row[o] > 0.0 || row[o + 1] > 0.0 || row[o + 2] > 0.0;
                let mut nmax = 0usize; // outermost annulus with any light on either side
                for n in (0..ABEL_R).rev() { if lit(cr(n)) || lit(cl(n)) { nmax = n + 1; break; } }
                for m in 0..nmax {
                    let wts = &abel[(m * ABEL_R) * 2..(m * ABEL_R + nmax) * 2];
                    let (mut rr, mut rg, mut rb, mut lr, mut lg, mut lb) = (0f32, 0f32, 0f32, 0f32, 0f32, 0f32);
                    for n in m..nmax {
                        let (wo, wx) = (wts[n * 2], wts[n * 2 + 1]);
                        let (or, ol) = (cr(n), cl(n));
                        rr += wo * row[or] + wx * row[ol]; rg += wo * row[or + 1] + wx * row[ol + 1]; rb += wo * row[or + 2] + wx * row[ol + 2];
                        lr += wo * row[ol] + wx * row[or]; lg += wo * row[ol + 1] + wx * row[or + 1]; lb += wo * row[ol + 2] + wx * row[or + 2];
                    }
                    let q = m * 6;
                    rowbuf[q] = rr; rowbuf[q + 1] = rg; rowbuf[q + 2] = rb; rowbuf[q + 3] = lr; rowbuf[q + 4] = lg; rowbuf[q + 5] = lb;
                }
                for m in 0..nmax {
                    let (or, ol, q) = (cr(m), cl(m), m * 6);
                    row[or] = rowbuf[q]; row[or + 1] = rowbuf[q + 1]; row[or + 2] = rowbuf[q + 2];
                    row[ol] = rowbuf[q + 3]; row[ol + 1] = rowbuf[q + 4]; row[ol + 2] = rowbuf[q + 5];
                }
            }
        }
        // 2x upsample in linear light with a separable Catmull-Rom (bicubic) kernel: blend four source rows once per
        // output row, then four columns per pixel from a precomputed tap table. The camera view then goes through the
        // tone table, so the bright core is clipped at the upsampled resolution rather than at the cell grid.
        let tone = view == 0;
        let (vmax, scale) = if tone { (ENC_VMAX, (ENC_N - 1) as f32 / ENC_VMAX) } else { (1.0, 255.0) };
        let Sim { taps, rowbuf, .. } = self;
        for y in 0..UH {
            let (rows, wy) = cubic_taps(y, H);
            if !(row_lit[rows[0]] || row_lit[rows[1]] || row_lit[rows[2]] || row_lit[rows[3]]) {
                for px in pix[y * UW * 4..(y + 1) * UW * 4].chunks_exact_mut(4) { px[0] = 0; px[1] = 0; px[2] = 0; px[3] = 255; }
                continue;
            }
            let r = [&lin[rows[0] * W * 3..(rows[0] + 1) * W * 3], &lin[rows[1] * W * 3..(rows[1] + 1) * W * 3],
                     &lin[rows[2] * W * 3..(rows[2] + 1) * W * 3], &lin[rows[3] * W * 3..(rows[3] + 1) * W * 3]];
            for i in 0..W * 3 { rowbuf[i] = wy[0] * r[0][i] + wy[1] * r[1][i] + wy[2] * r[2][i] + wy[3] * r[3][i]; }
            let out = &mut pix[y * UW * 4..(y + 1) * UW * 4];
            for x in 0..UW {
                let (q, wx) = taps[x];
                unsafe {
                    for c in 0..3 {
                        let v = wx[0] * *rowbuf.get_unchecked(q[0] as usize + c) + wx[1] * *rowbuf.get_unchecked(q[1] as usize + c)
                            + wx[2] * *rowbuf.get_unchecked(q[2] as usize + c) + wx[3] * *rowbuf.get_unchecked(q[3] as usize + c);
                        let v = v.max(0.0).min(vmax) * scale;
                        *out.get_unchecked_mut(x * 4 + c) = if tone { *enc.get_unchecked(v as usize) } else { v as u8 };
                    }
                    *out.get_unchecked_mut(x * 4 + 3) = 255;
                }
            }
        }
        let st = &mut self.stats;
        st[0] = sx * CELL_AREA; st[1] = sy * CELL_AREA; st[2] = sz * CELL_AREA; // summed light per unit area
        st[3] = if ws > 1e-3 { wt / ws } else { 0.0 };
        if sy > 1e-3 { st[4] = cx / sy; st[5] = cy / sy; } else if st[4] == 0.0 { st[4] = HEAD_X as f32; st[5] = 120.0; }
        st[6] = HEAD0 + self.burned;
        st[7] = self.phase as u32 as f32;
        st[8] = self.h; st[9] = self.ember; st[10] = self.burned; st[11] = self.since;
        st[12] = self.qrel; st[13] = self.qrad; st[14] = self.sparks.len().min(MAX_SPARKS) as f32;
        st[15] = self.time; st[16] = self.sim_ms;
        for (n, p) in self.sparks.iter().take(MAX_SPARKS).enumerate() {
            self.spark_buf[n * 5..n * 5 + 5].copy_from_slice(&[p.x, p.y, p.px, p.py, p.t]);
        }
    }

    /// Linear sRGB of an opaque grey body at temperature t, after white balance.
    pub fn grey_rgb(&mut self, t: f32, gain: f32) {
        let c = look(&self.grey, t);
        let m = &self.m;
        self.scratch[0] = (m[0] * c[0] + m[1] * c[1] + m[2] * c[2]) * gain;
        self.scratch[1] = (m[3] * c[0] + m[4] * c[1] + m[5] * c[2]) * gain;
        self.scratch[2] = (m[6] * c[0] + m[7] * c[1] + m[8] * c[2]) * gain;
    }

    // accessors used by native tests
    pub fn set_solver(&mut self, p: usize) { self.p_iters = p.max(1); }
    pub fn set_diffusion(&mut self, alpha0: f32, nu0: f32) { self.alpha0 = alpha0.max(0.0); self.nu0 = nu0.max(0.0); }
    pub fn set_fuel(&mut self, fuel: f32) { self.fuel = fuel.clamp(0.0, 1.0); }
    pub fn set_soot(&mut self, scale: f32) { self.soot_scale = scale.max(0.0); }
    pub fn set_wick(&mut self, curl: f32, supply_sigma: f32) { self.curl = curl.clamp(-20.0, 20.0); self.supply_sigma = supply_sigma.clamp(0.0, 0.5); }
    pub fn set_draft_model(&mut self, sigma: f32, tau: f32, coupling: f32) { self.draft_sigma = sigma.max(0.0); self.draft_tau = tau.max(0.05); self.draft_coupling = coupling.max(0.0); }
    pub fn set_krad(&mut self, scale: f32) { self.krad = KRAD * scale.max(0.0); }
    pub fn set_chem(&mut self, a_scale: f32, q_scale: f32) { self.rate = Self::rate_table(a_scale.max(0.01) as f64); self.q = Q * q_scale.max(0.1); }
    /// RMS over interior cells of the projection residual: divergence minus the thermal-expansion target.
    pub fn divergence_rms(&self) -> f32 {
        let (u, v, r, t) = (&self.u, &self.v, &self.r, &self.t);
        let (mut acc, mut n) = (0.0f64, 0usize);
        for j in 1..H - 1 {
            for i in 1..W - 1 {
                let k = i + j * W;
                let d = 0.5 * (u[k + 1] - u[k - 1] + v[k + W] - v[k - W]) - EXPAND * Q * r[k] / t[k];
                acc += (d as f64) * (d as f64);
                n += 1;
            }
        }
        (acc / n as f64).sqrt() as f32
    }
    pub fn phase_code(&self) -> u32 { self.phase as u32 }
    pub fn field_t(&self) -> &[f32] { &self.t }
    pub fn field_s(&self) -> &[f32] { &self.s }
    pub fn field_f(&self) -> &[f32] { &self.f }
    pub fn field_v(&self) -> &[f32] { &self.v }
}

/// Share of soot radiation that falls between 380 and 780 nm.
pub fn visible_share(t: f64) -> f64 {
    let (mut tot, mut vis) = (0.0, 0.0);
    let mut ll = 150f64.ln();
    while ll < 60000f64.ln() {
        let l = ll.exp();
        let e = planck(l, t) / l * l * 0.01;
        tot += e;
        if (380.0..=780.0).contains(&l) { vis += e; }
        ll += 0.01;
    }
    vis / tot
}

// ---------------------------------------------------------------- C ABI for JS
static mut SIM: Option<Sim> = None;
#[inline(always)]
fn sim() -> &'static mut Sim { unsafe { SIM.as_mut().unwrap_unchecked() } }

#[no_mangle] pub extern "C" fn fl_init() { unsafe { SIM = Some(Sim::new()); } }
#[no_mangle] pub extern "C" fn fl_seed(s: u32) { sim().rng = Rng(s | 1); }
#[no_mangle] pub extern "C" fn fl_reset() { sim().reset(); }
#[no_mangle] pub extern "C" fn fl_step(dt: f32) { sim().step(dt); }
#[no_mangle] pub extern "C" fn fl_set_sim_ms(ms: f32) { sim().sim_ms = ms; }
#[no_mangle] pub extern "C" fn fl_render() { sim().render(); }
#[no_mangle] pub extern "C" fn fl_strike(sparks: i32) -> i32 { sim().strike(sparks != 0) as i32 }
#[no_mangle] pub extern "C" fn fl_blow(speed: f32, dur: f32) { sim().blow(speed, dur); }
#[no_mangle] pub extern "C" fn fl_poke(x: f32, y: f32, vx: f32, vy: f32) { sim().poke(x, y, vx, vy); }
#[no_mangle] pub extern "C" fn fl_set_room(o2: f32, g: f32, swirl: f32, draft: f32) {
    let s = sim(); s.o2 = o2; s.g = g; s.swirl = swirl; s.draft_k = draft;
}
#[no_mangle] pub extern "C" fn fl_set_camera(expo: f32, wb_k: f32, chemi_gain: f32) {
    let s = sim(); s.expo = expo; s.chemi_gain = chemi_gain; s.set_wb(wb_k);
}
#[no_mangle] pub extern "C" fn fl_set_view(v: u32) { sim().view = v; }
#[no_mangle] pub extern "C" fn fl_set_solver(p: u32) { sim().set_solver(p as usize); }
#[no_mangle] pub extern "C" fn fl_set_diffusion(alpha: f32, nu: f32) { sim().set_diffusion(alpha, nu); }
#[no_mangle] pub extern "C" fn fl_set_fuel(fuel: f32) { sim().set_fuel(fuel); }
#[no_mangle] pub extern "C" fn fl_set_soot(scale: f32) { sim().set_soot(scale); }
#[no_mangle] pub extern "C" fn fl_set_wick(curl: f32, supply_sigma: f32) { sim().set_wick(curl, supply_sigma); }
#[no_mangle] pub extern "C" fn fl_set_draft_model(sigma: f32, tau: f32, coupling: f32) { sim().set_draft_model(sigma, tau, coupling); }
#[no_mangle] pub extern "C" fn fl_set_krad(scale: f32) { sim().set_krad(scale); }
#[no_mangle] pub extern "C" fn fl_set_chem(a_scale: f32, q_scale: f32) { sim().set_chem(a_scale, q_scale); }
#[no_mangle] pub extern "C" fn fl_grey_rgb(t: f32, gain: f32) { sim().grey_rgb(t, gain); }
#[no_mangle] pub extern "C" fn fl_visible_share(t: f32) -> f32 { visible_share(t as f64) as f32 }
#[no_mangle] pub extern "C" fn fl_pixels() -> *const u8 { sim().pix.as_ptr() }
#[no_mangle] pub extern "C" fn fl_stats() -> *const f32 { sim().stats.as_ptr() }
#[no_mangle] pub extern "C" fn fl_sparks() -> *const f32 { sim().spark_buf.as_ptr() }
#[no_mangle] pub extern "C" fn fl_matrix() -> *const f32 { sim().m.as_ptr() }
#[no_mangle] pub extern "C" fn fl_scratch() -> *const f32 { sim().scratch.as_ptr() }
#[no_mangle] pub extern "C" fn fl_dims() -> u32 { ((W as u32) << 16) | H as u32 }
#[no_mangle] pub extern "C" fn fl_updims() -> u32 { ((UW as u32) << 16) | UH as u32 }
#[no_mangle] pub extern "C" fn fl_head_x() -> u32 { HEAD_X as u32 }

// ---------------------------------------------------------------- native profiling helper

#[cfg(not(target_arch = "wasm32"))]
impl Sim {
    pub fn profile(&mut self, reps: usize) -> Vec<(&'static str, f64)> {
        use std::time::Instant;
        let dt = SUB_DT;
        let mut acc = [0f64; 6];
        for _ in 0..reps {
            let t0 = Instant::now(); self.forces(dt);
            let t1 = Instant::now(); self.wick(dt);
            let t2 = Instant::now(); self.transport(dt);
            let t3 = Instant::now(); self.entrain(dt);
            let t4 = Instant::now(); self.chemistry(dt);
            let t5 = Instant::now(); self.render();
            let t6 = Instant::now();
            for (a, d) in acc.iter_mut().zip([t1 - t0, t2 - t1, t3 - t2, t4 - t3, t5 - t4, t6 - t5]) { *a += d.as_secs_f64() * 1e3; }
        }
        let names = ["forces", "wick", "transport: project + advect + diffuse", "entrainment", "chemistry + radiation", "render, per frame"];
        let mut out: Vec<(&'static str, f64)> = names.iter().zip(acc).map(|(n, a)| (*n, a / reps as f64)).collect();
        let sub: f64 = out[..5].iter().map(|x| x.1).sum();
        out.push(("substep total", sub));
        let t0 = Instant::now(); for _ in 0..reps { self.project(); } out.push(("  of which project", t0.elapsed().as_secs_f64() * 1e3 / reps as f64));
        let t0 = Instant::now(); for _ in 0..reps { let Sim { t, dv, gx, gy, .. } = self; diffuse_adi(t, gx, gy, ALPHA0, dv); } out.push(("  of which diffusion, one field", t0.elapsed().as_secs_f64() * 1e3 / reps as f64));
        out
    }
}

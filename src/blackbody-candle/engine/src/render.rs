//! What the candle looks like: Planck's law through the CIE 1931 observer per cell, the line-of-sight projection of
//! the slice, the camera's white balance, and the readings. The physics is `physics`; this only reads it.
use crate::physics::{Phase, Sim, CELL_AREA, H, HEAD0, HEAD_X, N, W};

// ---------------------------------------------------------------- render constants
const ABEL_R: usize = HEAD_X; // half-width of the slice about the wick axis, cells
const ABEL_NORM: f32 = 1.0 / 20.0; // divides the projected intensity by a typical chord (20 cells) so exposure values stay sensible
const BLOOM_DS: usize = 4; // the bloom is a Gaussian of the linear image at 1/4 resolution: a camera's veiling glare
pub const BW: usize = W / BLOOM_DS;
pub const BH: usize = H / BLOOM_DS;
const BLOOM_SIGMA: f32 = 1.5; // in bloom pixels: 1.5 mm
const NSTATS: usize = 16;
const C2: f64 = 1.4387769e7; // nm*K
const LT_MIN: f32 = 300.0;
const LT_MAX: f32 = 7000.0;
const LT_STEP: f32 = 5.0;
const NT: usize = ((LT_MAX - LT_MIN) / LT_STEP) as usize + 1;

// ---------------------------------------------------------------- spectral tables
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
fn mul3(a: &[f64; 9], b: &[f64; 9]) -> [f64; 9] {
    let mut r = [0.0; 9];
    for i in 0..3 { for j in 0..3 { r[i * 3 + j] = a[i * 3] * b[j] + a[i * 3 + 1] * b[3 + j] + a[i * 3 + 2] * b[6 + j]; } }
    r
}

// ---------------------------------------------------------------- the temperature view's colour map
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

// ---------------------------------------------------------------- the renderer
pub struct Renderer {
    soot: Vec<f32>, grey: Vec<f32>, chemi: [f32; 3], m: [f32; 9],
    abel: Vec<f32>, // line-of-sight weights, two per (ray m, annulus n): own side and other side of the slice
    pub expo: f32, pub chemi_gain: f32, pub view: u32,
    lin: Vec<f32>, rowbuf: Vec<f32>, bloom: Vec<f32>, bloom0: Vec<f32>, bloom_w: Vec<f32>,
    pub stats: [f32; NSTATS], scratch: [f32; 4],
}

impl Renderer {
    pub fn new() -> Renderer {
        // spectral lookup tables (f64 while integrating)
        let mut soot = vec![0f64; NT * 3];
        let mut grey = vec![0f64; NT * 3];
        for n in 0..NT {
            let t = LT_MIN as f64 + n as f64 * LT_STEP as f64;
            let s = integrate(|l| planck(l, t) / l); // soot: emissivity ~ 1/lambda
            let b = integrate(|l| planck(l, t)); // grey body: the glowing wick
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
        // Abel projection of the slice: the camera sees an optically thin, roughly axisymmetric flame, so the intensity at
        // offset x from the axis is the integral of the slice emission along the chord through all annuli at r >= x.
        // Cell n on one side of the axis is the annulus [n, n+1]; the ray through cell m is at x = m + 0.5. The flame
        // sways, so the two halves of the slice differ: along the chord the azimuth runs from the ray's own side round
        // to the far side, and the emission is the two halves mixed by (1 +- x/r)/2. Even index: own side; odd: other.
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
        let mut r = Renderer {
            soot, grey, chemi, m: [0.0; 9], abel, expo: 1.0, chemi_gain: 0.00022, view: 0,
            lin: vec![0.0; N * 3], rowbuf: vec![0.0; W * 3],
            bloom: vec![0.0; BW * BH * 3], bloom0: vec![0.0; BW * BH * 3],
            bloom_w: (-4..=4).map(|i| (-(i * i) as f32 / (2.0 * BLOOM_SIGMA * BLOOM_SIGMA)).exp()).collect::<Vec<f32>>(),
            stats: [0.0; NSTATS], scratch: [0.0; 4],
        };
        r.set_wb(2800.0);
        r
    }

    pub fn linear(&self) -> &[f32] { &self.lin }
    pub fn bloom(&self) -> &[f32] { &self.bloom }
    pub fn matrix(&self) -> &[f32; 9] { &self.m }
    pub fn scratch(&self) -> &[f32; 4] { &self.scratch }

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

    /// Radiance of every cell in linear light (the camera view projected along the line of sight), the bloom, and the
    /// summary stats for the JS side. A GPU presenter resamples and tone-maps `lin` per device pixel.
    pub fn render(&mut self, sim: &Sim) {
        let (tt, ss, rr, fu) = (&sim.t[..], &sim.s[..], &sim.r[..], &sim.f[..]);
        let Renderer { lin, soot, m, .. } = self;
        let (expo, cg, view, ch) = (self.expo, self.chemi_gain, self.view, self.chemi);
        let smoke_f = sim.phase == Phase::Out;
        let (mut sx, mut sy, mut sz, mut wt, mut ws, mut cx, mut cy) = (0f32, 0f32, 0f32, 0f32, 0f32, 0f32, 0f32);
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
        // camera view: replace each row of the slice by its line-of-sight projection. A swaying flame is symmetric
        // about its own bent axis, not the wick's, so each row is projected about its centre of light.
        if view == 0 {
            let Renderer { abel, rowbuf, .. } = self;
            for j in 0..H {
                if !row_lit[j] { continue; }
                let row = &mut lin[j * W * 3..(j + 1) * W * 3];
                let (mut wsum, mut wx, mut xlo, mut xhi) = (0f32, 0f32, W, 0usize);
                for i in 0..W { let o = i * 3; let l = row[o] + row[o + 1] + row[o + 2]; if l > 0.0 { wsum += l; wx += l * (i as f32 + 0.5); xlo = xlo.min(i); xhi = i; } }
                if wsum <= 0.0 { continue; }
                let ax = ((wx / wsum).round() as usize).clamp(1, W - 1);
                // annulus n is column ax + n on the right and ax - 1 - n on the left: the axis is the edge between them
                let (cr, cl) = (|n: usize| if ax + n < W { Some((ax + n) * 3) } else { None }, |n: usize| if n < ax { Some((ax - 1 - n) * 3) } else { None });
                let nmax = ((xhi + 1 - ax).max(ax - xlo)).min(ABEL_R);
                for m in 0..nmax {
                    let wts = &abel[(m * ABEL_R) * 2..(m * ABEL_R + nmax) * 2];
                    let (mut rr, mut rg, mut rb, mut lr, mut lg, mut lb) = (0f32, 0f32, 0f32, 0f32, 0f32, 0f32);
                    for n in m..nmax {
                        let (wo, wx) = (wts[n * 2], wts[n * 2 + 1]);
                        let (r1, g1, b1) = match cr(n) { Some(o) => (row[o], row[o + 1], row[o + 2]), None => (0.0, 0.0, 0.0) };
                        let (r2, g2, b2) = match cl(n) { Some(o) => (row[o], row[o + 1], row[o + 2]), None => (0.0, 0.0, 0.0) };
                        rr += wo * r1 + wx * r2; rg += wo * g1 + wx * g2; rb += wo * b1 + wx * b2;
                        lr += wo * r2 + wx * r1; lg += wo * g2 + wx * g1; lb += wo * b2 + wx * b1;
                    }
                    let q = m * 6;
                    rowbuf[q] = rr; rowbuf[q + 1] = rg; rowbuf[q + 2] = rb; rowbuf[q + 3] = lr; rowbuf[q + 4] = lg; rowbuf[q + 5] = lb;
                }
                for m in 0..nmax {
                    let q = m * 6;
                    if let Some(o) = cr(m) { row[o] = rowbuf[q]; row[o + 1] = rowbuf[q + 1]; row[o + 2] = rowbuf[q + 2]; }
                    if let Some(o) = cl(m) { row[o] = rowbuf[q + 3]; row[o + 1] = rowbuf[q + 4]; row[o + 2] = rowbuf[q + 5]; }
                }
            }
        }
        // the bloom: a Gaussian of the linear image at 1/4 resolution, box-downsampled then blurred separably
        {
            let Renderer { lin, bloom, bloom0, bloom_w, .. } = self;
            let inv = 1.0 / (BLOOM_DS * BLOOM_DS) as f32;
            for by in 0..BH {
                for bx in 0..BW {
                    let (mut r, mut g, mut b) = (0f32, 0f32, 0f32);
                    for dy in 0..BLOOM_DS { for dx in 0..BLOOM_DS { let o = ((by * BLOOM_DS + dy) * W + bx * BLOOM_DS + dx) * 3; r += lin[o]; g += lin[o + 1]; b += lin[o + 2]; } }
                    let o = (by * BW + bx) * 3;
                    bloom0[o] = r * inv; bloom0[o + 1] = g * inv; bloom0[o + 2] = b * inv;
                }
            }
            let wsum: f32 = bloom_w.iter().sum();
            for by in 0..BH {
                for bx in 0..BW {
                    let (mut r, mut g, mut b) = (0f32, 0f32, 0f32);
                    for (t, w) in bloom_w.iter().enumerate() { let sx = (bx as i32 + t as i32 - 4).clamp(0, BW as i32 - 1) as usize; let o = (by * BW + sx) * 3; r += w * bloom0[o]; g += w * bloom0[o + 1]; b += w * bloom0[o + 2]; }
                    let o = (by * BW + bx) * 3; bloom[o] = r / wsum; bloom[o + 1] = g / wsum; bloom[o + 2] = b / wsum;
                }
            }
            for by in 0..BH {
                for bx in 0..BW {
                    let (mut r, mut g, mut b) = (0f32, 0f32, 0f32);
                    for (t, w) in bloom_w.iter().enumerate() { let sy = (by as i32 + t as i32 - 4).clamp(0, BH as i32 - 1) as usize; let o = (sy * BW + bx) * 3; r += w * bloom[o]; g += w * bloom[o + 1]; b += w * bloom[o + 2]; }
                    let o = (by * BW + bx) * 3; bloom0[o] = r / wsum; bloom0[o + 1] = g / wsum; bloom0[o + 2] = b / wsum;
                }
            }
            std::mem::swap(bloom, bloom0);
        }
        let st = &mut self.stats;
        st[0] = sx * CELL_AREA; st[1] = sy * CELL_AREA; st[2] = sz * CELL_AREA; // summed light per unit area
        st[3] = if ws > 1e-3 { wt / ws } else { 0.0 };
        if sy > 1e-3 { st[4] = cx / sy; st[5] = cy / sy; } else if st[4] == 0.0 { st[4] = HEAD_X as f32; st[5] = 120.0; }
        st[6] = HEAD0;
        st[7] = sim.phase as u32 as f32;
        st[8] = sim.h; st[9] = sim.ember; st[10] = sim.since;
        st[11] = sim.qrel; st[12] = sim.qrad; st[13] = sim.time;
    }

    /// Linear sRGB of an opaque grey body at temperature t, after white balance.
    pub fn grey_rgb(&mut self, t: f32, gain: f32) {
        let c = look(&self.grey, t);
        let m = &self.m;
        self.scratch[0] = (m[0] * c[0] + m[1] * c[1] + m[2] * c[2]) * gain;
        self.scratch[1] = (m[3] * c[0] + m[4] * c[1] + m[5] * c[2]) * gain;
        self.scratch[2] = (m[6] * c[0] + m[7] * c[1] + m[8] * c[2]) * gain;
    }

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

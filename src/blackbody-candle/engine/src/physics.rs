//! The candle: a reacting, buoyant 2D flow of wax vapour, air, heat and soot on a 0.25 mm grid, and the wick that
//! feeds it. Everything here is physics; what it looks like is `render`.
use std::f32::consts::PI;

// ---------------------------------------------------------------- grid & physics constants
pub const W: usize = 168;
pub const H: usize = 280;
pub const N: usize = W * H;
pub const DX: f32 = 0.25; // mm per cell (42 x 70 mm window)
pub const CELL_AREA: f32 = DX * DX / 0.25; // cell area relative to a 0.5 mm grid: summed light and heat are per unit area
pub const TA: f32 = 300.0; // room temperature, K
const G: f32 = 9810.0 / DX; // gravity, cells/s^2
const A: f32 = 1e5; // one-step chemistry: pre-exponential, 1/s
const EA: f32 = 6000.0; // activation temperature, K
const STO: f32 = 6.0; // oxygen used per unit fuel
const Q: f32 = 11300.0; // temperature rise per unit fuel burned, K: a stoichiometric mixture reaches ~2,200 K, the adiabatic flame temperature of hydrocarbons in air
const SF: f32 = 1.45e7; // soot inception pre-exponential
const ESF: f32 = 20000.0; // soot inception activation temperature, K (soot models use ~21,000)
const SO: f32 = 2e6; // soot oxidation pre-exponential: closes the luminous tip
const ESO: f32 = 11000.0; // its activation temperature, K
const OHP: f32 = 0.012; // OH proxy for soot oxidation
const KRAD: f32 = 5e-9; // optically thin radiative loss
const ALPHA0: f32 = 22.0; // thermal diffusivity of air at 300 K, mm^2/s
const D_EXP: f32 = 1.7; // gas diffusivities scale as (T/300)^1.7: ~400 mm^2/s in the flame
const PR: f32 = 0.71; // Prandtl number of air: kinematic viscosity = PR * alpha
const LE_F: f32 = 3.0; // Lewis number of the fuel vapour: heavy hydrocarbons diffuse about 3x slower than heat
const LE_O: f32 = 1.0; // oxygen diffuses about like heat
const ENT: f32 = 0.25; // out-of-plane entrainment: a round flame draws air, and spreads its gases, into the third dimension
const ENT_R: f32 = 20.0; // flame radius the entrainment rate is scaled by, cells (5 mm)
const EXPAND: f32 = 0.5; // thermal expansion in the pressure solve
const FUEL: f32 = 0.9; // wax vapour supply of the wick, in fuel-fraction units; sets the flame size
const TIP_R: f32 = 14.0; // radius of the hot surface around the wick tip, cells (3.5 mm): what the flame anchors to
const WICK_COLD: f32 = 900.0; // a gust that cools the glowing wick below this cannot be survived: a 5 m/s puff for 0.4 s reaches ~780 K, a breeze ~1,100
const WICK_CURL: f32 = 6.0; // a braided wick curls so its tip burns in the sheath: the vapour source sits this many cells to the side
const SUPPLY_SIGMA: f32 = 0.12; // the capillary feed and melt pool fluctuate: relative noise on the vapour supply
const SUPPLY_TAU: f32 = 0.3; // and its correlation time, s
const DRAFT_SIGMA: f32 = 130.0; // room air: standard deviation of the wandering wind, mm/s, at draft 1x. The hot plume shields itself; ~40 % reaches the flame
const DRAFT_TAU: f32 = 0.8; // its correlation time, s
const DRAFT_COUPLING: f32 = 3.0; // rate at which the air in the window follows the room wind, 1/s
pub const HEAD_X: usize = 84; // wick column
pub const HEAD0: f32 = 156.0; // wick tip row: the flame anchors here
const WAX_TOP: usize = 174; // first row of solid wax below the wick
const WAX_HALF: usize = 20; // half-width of the wax column, cells (10 mm across)
const SUB_DT: f32 = 1.0 / 240.0; // the substep: at 1/120 the split between transport and chemistry rang at a period of two steps and striped the core with soot bands
const MAX_FRAME_SIM: f32 = 0.025; // at most this much simulated time per frame: a slow frame rate slows the flame rather than stalling the page
const RT_MIN: f32 = TA; // the per-substep rate tables cover the clamped temperature range, 1 K apart
const RT_N: usize = 2301; // 300..=2600 K
const P_ITERS: usize = 16; // pressure sweeps; the flame is insensitive beyond this

// ---------------------------------------------------------------- per-substep rate tables
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

// ---------------------------------------------------------------- small helpers
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

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Phase { Ready = 0, Lit = 1, Out = 2 }

// ---------------------------------------------------------------- field kernels
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

/// Semi-Lagrangian advection. All six fields ride the same velocity, so each cell is traced back once, with the
/// midpoint rule (a straight back-trace leaves a step-locked pattern in sheared flow), and every field is sampled there.
fn advect_all(dst: [&mut [f32]; 6], src: [&[f32]; 6], amb: [Option<f32>; 6], dt: f32, uu: &[f32], vv: &[f32]) {
    let [d0, d1, d2, d3, d4, d5] = dst;
    let (xmax, ymax) = (W as f32 - 1.5, H as f32 - 1.5);
    let vel_at = |x: f32, y: f32| -> (f32, f32) {
        let (i0, j0) = (x as usize, y as usize);
        let (a, b) = (x - i0 as f32, y - j0 as f32);
        let q = i0 + j0 * W;
        let (w00, w10, w01, w11) = ((1.0 - a) * (1.0 - b), a * (1.0 - b), (1.0 - a) * b, a * b);
        unsafe {
            (w00 * uu.get_unchecked(q) + w10 * uu.get_unchecked(q + 1) + w01 * uu.get_unchecked(q + W) + w11 * uu.get_unchecked(q + W + 1),
             w00 * vv.get_unchecked(q) + w10 * vv.get_unchecked(q + 1) + w01 * vv.get_unchecked(q + W) + w11 * vv.get_unchecked(q + W + 1))
        }
    };
    for j in 1..H - 1 {
        for i in 1..W - 1 {
            let k = i + j * W;
            unsafe {
                let (xm, ym) = ((i as f32 - 0.5 * dt * uu.get_unchecked(k)).clamp(0.5, xmax), (j as f32 - 0.5 * dt * vv.get_unchecked(k)).clamp(0.5, ymax));
                let (um, vm) = vel_at(xm, ym);
                let x = (i as f32 - dt * um).clamp(0.5, xmax);
                let y = (j as f32 - dt * vm).clamp(0.5, ymax);
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
#[derive(Clone)]
pub struct Sim {
    pub(crate) u: Vec<f32>, pub(crate) v: Vec<f32>, u0: Vec<f32>, v0: Vec<f32>,
    pub(crate) t: Vec<f32>, t0: Vec<f32>, pub(crate) f: Vec<f32>, f0: Vec<f32>,
    pub(crate) o: Vec<f32>, o0: Vec<f32>, pub(crate) s: Vec<f32>, s0: Vec<f32>,
    pub(crate) r: Vec<f32>, p: Vec<f32>, dv: Vec<f32>, cu: Vec<f32>, solid: Vec<u8>,
    gtab: Vec<f32>, gx: Vec<f32>, gy: Vec<f32>, // (T/300)^D_EXP per K; face coefficients for the current substep
    rate: Vec<f32>, // per K: burn fraction per substep, soot inception per unit fuel, soot oxidation coefficient
    // room
    pub(crate) o2: f32, pub(crate) g: f32, pub(crate) swirl: f32, pub(crate) draft_k: f32,
    // candle state
    pub(crate) phase: Phase, pub(crate) time: f32, pub(crate) h: f32, charge: f32, ignite: f32, pub(crate) ember: f32,
    out_timer: f32, gust: f32, gust_v: f32, draft: f32, supply: f32, pub(crate) qrel: f32, pub(crate) qrad: f32, pub(crate) since: f32,
    rng: Rng,
}

impl Sim {
    pub fn new() -> Sim {
        // the three Arrhenius rates, integrated over one substep, as functions of temperature: the hot loop does one
        // table lookup instead of exp calls
        let mut rate = vec![0f32; RT_N * 3];
        for n in 0..RT_N {
            let t = RT_MIN as f64 + n as f64;
            let dt = SUB_DT as f64;
            rate[n * 3] = (1.0 - (-A as f64 * (-EA as f64 / t).exp() * dt).exp()) as f32; // fraction of the limiting reactant burned
            rate[n * 3 + 1] = (dt * SF as f64 * (-ESF as f64 / t).exp()) as f32; // soot formed per unit fuel
            rate[n * 3 + 2] = (dt * SO as f64 * (-ESO as f64 / t).exp()) as f32; // soot oxidation coefficient, times (O2 + OH proxy)
        }
        let z = || vec![0f32; N];
        let mut sim = Sim {
            u: z(), v: z(), u0: z(), v0: z(), t: z(), t0: z(), f: z(), f0: z(), o: z(), o0: z(), s: z(), s0: z(),
            r: z(), p: z(), dv: z(), cu: z(), solid: vec![0; N],
            gtab: (0..RT_N).map(|n| ((RT_MIN + n as f32) / TA).powf(D_EXP)).collect(), gx: z(), gy: z(),
            rate,
            o2: 0.21, g: 1.0, swirl: 1.0, draft_k: 1.0,
            phase: Phase::Ready, time: 0.0, h: 0.0, charge: 0.0, ignite: 0.0, ember: TA,
            out_timer: 0.0, gust: 0.0, gust_v: 0.0, draft: 0.0, supply: 0.0, qrel: 0.0, qrad: 0.0, since: 0.0,
            rng: Rng(0x9e3779b9),
        };
        sim.reset();
        sim
    }

    pub fn seed(&mut self, s: u32) { self.rng = Rng(s | 1); }
    pub fn set_room(&mut self, o2: f32, g: f32, swirl: f32, draft: f32) { self.o2 = o2; self.g = g; self.swirl = swirl; self.draft_k = draft; }

    pub fn reset(&mut self) {
        self.u.fill(0.0); self.v.fill(0.0); self.t.fill(TA); self.f.fill(0.0);
        self.s.fill(0.0); self.r.fill(0.0); self.p.fill(0.0);
        self.o.fill(self.o2 / 0.21);
        self.phase = Phase::Ready;
        self.time = 0.0; self.h = 0.0; self.charge = 0.0; self.ignite = 0.0;
        self.ember = TA; self.out_timer = 0.0; self.gust = 0.0; self.gust_v = 0.0; self.draft = 0.0; self.supply = 0.0;
        self.qrel = 0.0; self.qrad = 0.0; self.since = 0.0;
        self.solid.fill(0);
        for j in WAX_TOP..H { for i in HEAD_X - WAX_HALF..HEAD_X + WAX_HALF { self.solid[i + j * W] = 1; } }
    }

    /// Wax vapour leaving the wick, driven by the heat feedback from the flame and a noisy capillary feed.
    fn src_strength(&self) -> f32 {
        match self.phase {
            Phase::Lit => (0.2 + 0.8 * self.h + self.charge * 0.6).min(1.0) * FUEL * (1.0 + self.supply),
            // just blown out: the hot wick keeps vaporising wax for a few seconds, which shows as smoke
            Phase::Out => 0.25 * (-self.since / 2.5).exp(),
            Phase::Ready => 0.0,
        }
    }

    /// Hottest gas touching the wick: is the flame still attached?
    fn near_t(&self) -> f32 {
        let hy = HEAD0 as i32;
        let mut m = TA;
        for j in -24..=4i32 {
            for i in -16..=16i32 {
                let k = (HEAD_X as i32 + i + (hy + j) * W as i32) as usize;
                if self.t[k] > m { m = self.t[k]; }
            }
        }
        m
    }

    /// Make the velocity divergence-free (up to the thermal expansion): Chorin/Stam projection, red-black Gauss-Seidel.
    fn project(&mut self) {
        let Sim { u, v, p, dv, r, t, solid, .. } = self;
        for j in 1..H - 1 {
            for i in 1..W - 1 {
                // thermal expansion: burning gas swells, so the flow is not divergence-free where heat is released
                let k = i + j * W;
                dv[k] = 0.5 * (u[k + 1] - u[k - 1] + v[k + W] - v[k - W]) - EXPAND * Q * r[k] / t[k];
            }
        }
        p.fill(0.0);
        for it in 0..P_ITERS {
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
        let hy = HEAD0;
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
        // room draft: a slowly wandering air current (Ornstein-Uhlenbeck); the air in the window follows it, plus small eddies
        self.draft += (-self.draft / DRAFT_TAU) * dt + (2.0 * dt / DRAFT_TAU).sqrt() * self.rng.gauss() * self.draft_k * DRAFT_SIGMA / DX;
        let dr = self.draft;
        for k in 0..N { self.u[k] += (dr - self.u[k]) * DRAFT_COUPLING * dt; }
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
                    // an eddy is an event: its rate scales with dt (above), its impulse does not
                    self.u[k] += -(j as f32) * w * sg * (10.0 / 120.0);
                    self.v[k] += i as f32 * w * sg * (10.0 / 120.0);
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

    /// The wick: wax vapour leaves it, and the glowing tip heats the gas touching it.
    fn wick(&mut self, dt: f32) {
        let hy = HEAD0;
        let pyro = self.src_strength();
        let ts = self.ember.max(if self.phase == Phase::Out { TA + 200.0 * (-self.since / 2.5).exp() } else { TA });
        // the curled wick: its glowing tip, and most of the vapour, sit to one side of the axis
        let (cx_tip, cx_fuel) = (WICK_CURL, 0.7 * WICK_CURL);
        // the source relaxations are rates, calibrated at a 1/120 s step: gas touching the hot surface gets half-way per 1/120 s
        let steps = dt * 120.0;
        let heat_k = 1.0 - 0.5f32.powf(steps);
        for j in -16..=10i32 {
            for i in -14..=22i32 {
                let (dtip, df) = (i as f32 - cx_tip, i as f32 - cx_fuel);
                let ei = df * df / 120.0 + (j * j) as f32 / 96.0;
                let e_tip = (dtip * dtip + ((j + 1) * (j + 1)) as f32) / (TIP_R * TIP_R);
                if ei > 1.0 && e_tip > 1.0 { continue; }
                let k = (HEAD_X as i32 + i + ((hy + j as f32) as i32) * W as i32) as usize;
                if self.solid[k] != 0 { continue; }
                if e_tip < 1.0 && self.t[k] < ts { self.t[k] += (ts - self.t[k]) * heat_k * ((1.0 - e_tip) * 2.5).min(1.0); }
                let wi = ((1.0 - ei) * 2.0).clamp(0.0, 1.0);
                if wi > 0.0 {
                    if pyro > 0.0 {
                        self.f[k] = self.f[k].max(pyro * wi);
                        self.o[k] *= (1.0 - 0.9 * pyro.min(1.0) * wi).powf(steps);
                        self.v[k] = self.v[k].min(-120.0 * pyro * wi);
                        self.u[k] += df * 8.0 * pyro * wi * dt * 60.0;
                    }
                    if self.ignite > 0.0 { self.t[k] = self.t[k].max(2300.0); }
                    if self.charge > 0.0 { self.t[k] = (self.t[k] + 4000.0 * self.charge * dt).min(2600.0); }
                }
            }
        }
    }

    /// Make the velocity divergence-free, carry everything along it, then diffuse with temperature-dependent coefficients.
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
            let Sim { u, v, u0, v0, t, t0, f, f0, o, o0, s, s0, dv, cu, gtab, gx, gy, solid, .. } = self;
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
            let (alpha, nu) = (ALPHA0, PR * ALPHA0);
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

    /// Out-of-plane entrainment: a real flame is round, so air also arrives from the third dimension and the flame's
    /// gases spread into it. Each cell mixes toward room air at a rate proportional to |velocity| / flame radius.
    fn entrain(&mut self, dt: f32) {
        let oa = self.o2 / 0.21;
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

    /// One-step chemistry, soot formation and oxidation, optically thin radiation. Returns the heat-release rate.
    fn chemistry(&mut self, dt: f32) -> f32 {
        let oa = self.o2 / 0.21;
        let mut rsum = 0.0f32;
        let mut qrad = 0.0f32;
        {
            let Sim { t: tt, f: ff, o: oo, s: ss, r: rr, solid, rate, .. } = self;
            let smooth = 1.0 - 0.5f32.powf(dt * 120.0);
            for k in 0..N {
                let (mut t, mut f, mut o, mut s) = (tt[k], ff[k], oo[k], ss[k]);
                let mut r = 0.0;
                if t > 600.0 && f > 1e-5 && o > 1e-5 {
                    let kb = rate_at(rate, t, 0); // fraction of the limiting reactant that burns this substep
                    let lim = f.min(o / STO);
                    r = lim * kb;
                    f -= r; o -= STO * r; t += Q * r;
                }
                if t > 900.0 {
                    let [_, ks, ko] = rate3(rate, t);
                    if t > 1100.0 && f > 0.005 { s += ks * f; }
                    // O2 + OH (proxy) attack soot
                    if s > 1e-6 { s -= s * (1.0 - (-ko * (o + OHP)).exp()); }
                }
                // optically thin radiation. dT/dt = -k T^4 integrates exactly to T^-3 = T0^-3 + 3 k dt, which is what a sooty
                // cell cooling by 100 K in a step needs; the small T_inf^4 term stays explicit. Cold cells use one multiply.
                let (t3, kk) = (t * t * t, dt * KRAD * (s + 0.04));
                let ql = if kk * t3 > 2e-3 { t - (1.0 / t3 + 3.0 * kk).powf(-1.0 / 3.0) - kk * 8.1e9 } else { kk * (t3 * t - 8.1e9) };
                t -= ql; qrad += ql;
                // flush tiny values to zero: denormal floats are very slow on most CPUs
                let rn = rr[k] + (r / dt - rr[k]) * smooth;
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
        self.qrel = self.qrel * 0.98 + 0.02 * Q * rsum;
        self.qrad = self.qrad * 0.98 + 0.02 * qrad;
        rsum / dt
    }

    pub fn step(&mut self, frame_dt: f32) {
        let n = ((frame_dt / SUB_DT).round() as i32).clamp(1, (MAX_FRAME_SIM / SUB_DT).round() as i32);
        for _ in 0..n {
            let heat = self.substep(SUB_DT);
            let dt = SUB_DT;
            self.time += dt; self.since += dt;
            self.supply += (-self.supply / SUPPLY_TAU) * dt + (2.0 * dt / SUPPLY_TAU).sqrt() * self.rng.gauss() * SUPPLY_SIGMA;
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
                    let wick_cold = self.gust > 0.0 && self.ember < WICK_COLD;
                    if (self.since > 0.4 && self.out_timer > 0.25) || wick_cold { self.phase = Phase::Out; self.since = 0.0; }
                }
                Phase::Out => { self.ember = TA + (self.ember - TA) * (-dt / 2.2).exp(); self.h = 0.0; }
                Phase::Ready => {}
            }
        }
    }

    pub fn strike(&mut self) -> bool {
        if self.phase != Phase::Ready { return false; }
        self.phase = Phase::Lit; self.since = 0.0; self.ignite = 0.12; self.charge = 1.0; self.h = 0.5; self.ember = 1300.0;
        true
    }

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

    pub fn blow(&mut self, speed_ms: f32, dur: f32) { self.gust = dur; self.gust_v = speed_ms * 1000.0 / DX; }
}

// ---------------------------------------------------------------- native profiling helper
#[cfg(not(target_arch = "wasm32"))]
impl Sim {
    pub fn profile(&mut self, reps: usize) -> Vec<(&'static str, f64)> {
        use std::time::Instant;
        let dt = SUB_DT;
        let mut acc = [0f64; 5];
        for _ in 0..reps {
            let t0 = Instant::now(); self.forces(dt);
            let t1 = Instant::now(); self.wick(dt);
            let t2 = Instant::now(); self.transport(dt);
            let t3 = Instant::now(); self.entrain(dt);
            let t4 = Instant::now(); self.chemistry(dt);
            let t5 = Instant::now();
            for (a, d) in acc.iter_mut().zip([t1 - t0, t2 - t1, t3 - t2, t4 - t3, t5 - t4]) { *a += d.as_secs_f64() * 1e3; }
        }
        let names = ["forces", "wick", "transport: project + advect + diffuse", "entrainment", "chemistry + radiation"];
        let mut out: Vec<(&'static str, f64)> = names.iter().zip(acc).map(|(n, a)| (*n, a / reps as f64)).collect();
        let sub: f64 = out.iter().map(|x| x.1).sum();
        out.push(("substep total", sub));
        out
    }
}

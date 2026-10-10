# Handoff: Blackbody Candle

Context for whoever continues this, human or Claude. Read this first, then `README.md` and
`engine/README.md`.

## Paste this to start a new Claude session

> I'm continuing a project called Blackbody Candle: a physics-based match flame in the browser.
> The engine is Rust compiled to WebAssembly (`engine/`), the page is a single HTML file built by
> `tools/build.py` from `web/`. Please read `HANDOFF.md`, `README.md` and `engine/README.md`,
> then run `python3 tools/build.py`, `node tools/bench.js` and `node tools/snapshot.js` to confirm
> everything builds and the flame burns (steady soot temperature should be about 1,620 K).
> Then tell me what you found before changing anything.

## In the simple-demos repo (2026-10-09)

This folder is `src/blackbody-candle/` of the simple-demos site. The page sources moved from
`web/src/` to `web/`, and `tools/build.py` now writes the built page to
`public/blackbody-candle/index.html` at the repo root, which Cloudflare serves as-is. The wasm was
rebuilt here with rustc 1.98.1 and `rustup target add wasm32-unknown-unknown`; it is not
byte-identical to the original (built with 1.97.0 via `build-std`) but the physics agree, see
`tools/bench.js`. The template gained the site's head block (viewport, canonical, Open Graph) and
a footer link back to the demo index.

**Match → candle, JS engine dropped (2026-10-09).** The demo is now a candle, so the flame never
moves. The solid is a wax column (`WAX_TOP`, `WAX_HALF`) instead of a stick; the fuel source and
hot-surface model are unchanged and sit around a fixed wick tip at row 78. There is no burn-down
(`burned` stays 0, the 40-cell death is gone), no sparks on lighting, and after a blow-out the hot
wick keeps vaporising wax for a few seconds, which shows as smoke. `drawStick` became `drawCandle`
in `web/app.js`. The JavaScript reference engine (`core.js`) and the engine switch are gone: the
Rust engine is the only one, so the "change it in both" rule below no longer applies. Everything
below this line still says "match" where it describes history.

**0.25 mm grid, bicubic upsample (2026-10-10).** `W, H = 168, 280`, `DX = 0.25`. Every constant in cell
units doubled: wick position and wax (`HEAD_X 84`, `HEAD0 156`, `WAX_TOP 174`, `WAX_HALF 20`), the
source ellipses, the `near_t` box, eddy centre spread and radius, gust extent, `ENT_R 20`, poke radius
14, the wick's upward vapour speed 120 cells/s. `P_ITERS 32` and `D_ITERS 8` keep the solvers as
converged as 16/4 were on the coarse grid. `CELL_AREA` normalises the summed light (`stats[0..3]`) and
the heat sums, so the JS lighting gains and the blow-out threshold did not change. The render's 2x
upsample is a separable Catmull-Rom kernel now. Vorticity confinement `eps` was left at 24 cells/s,
which halves its physical strength; the finer grid dissipates less, so that is the Fedkiw scaling.
In `web/app.js` the canvas backing store is capped at 2x instead of 1.5x and the bloom layer is W/8.

**Solvers (2026-10-10).** Measured with `examples/solver.rs`: the flame is insensitive to the pressure
sweep count beyond 16 (`P_ITERS` is 16 again), and the residual has a floor at about 0.55x because the
wax and the boundary clamps are applied after the solve. The implicit diffusion delivered only ~23 % of
its nominal `DIFF = 420` and was in effect a blur whose width set the flame size. It is replaced by the
exact propagator of the diffusion equation: a separable Gaussian with variance 2·D·dt per axis
(`Blur`, `blur()` in lib.rs), with physical constants `ALPHA = 30` mm²/s for T, F, O and `NU = 15` for
u, v, tuned once so the 0.25 mm flame matches the 0.5 mm reference (1,641 K, 22 % radiant, 5.0 mm
wide). This makes the diffusion grid-independent. `fl_set_solver(p)` and `fl_set_diffusion(alpha, nu)`
are exported for experiments; `tools/snapshot.js` takes `PIT`, `ALPHA`, `NU`. The `DIFF` row in the
constants table below is history. Next physics step: temperature-dependent diffusivity, air goes from
22 mm²/s at 300 K to ~300 at 1500 K.

**Rate tables, T-dependent diffusion, recalibration (2026-10-10).** The three Arrhenius rates are
per-kelvin tables (`rate_table`), the entrainment uses a series for 1 − e^−x; the substep is split
into `forces / wick / transport / entrain / chemistry` and the native profiler times each. Diffusion
is now `diffuse_adi`: locally one-dimensional implicit sweeps with face coefficients from
`ALPHA0 = 22` mm²/s × (T/300)^1.7, Prandtl 0.71 for velocity, Lewis 3 for fuel and 1 for oxygen,
harmonic-mean faces so the wax is insulating. With physical diffusivities the old constants gave a
short, cool flame; the fix was physical too: `Q = 11300` (adiabatic flame temperature ~2,200 K
instead of 1,750) and `FUEL = 0.5`. Runtime knobs for tuning: `fl_set_diffusion(alpha0, nu0)`,
`fl_set_fuel`, `fl_set_soot(scale)`, `fl_set_krad(scale)`, `fl_set_chem(a_scale, q_scale)`, all
reachable from `tools/snapshot.js` through env vars. The render skips cells and rows with no light
(exact: they were zeros). Rows for `DIFF`, `Q`, and fuel scale in the constants table below are history.

**Soot inception and line of sight (2026-10-10).** With physical diffusion and Q the warm vapour core
made soot, and a soot-radiation relaxation cycle striped it (T and S oscillating in antiphase along
the axis, period ~11 cells; `examples/axis.rs` shows it). Fix: `ESF = 20000` K (soot models use
~21,000) with `SF = 1.45e7` so the sheath rate is unchanged; inception now stays in the hot sheath
and the core is dark, as in a real candle. A dark core looks hollow in a 2D slice, so the camera view
is now the Abel projection of the slice about the wick axis (`abel` weights, applied per lit row in
`render`), scaled by `ABEL_NORM`. The temperature, soot and reaction views still show the slice.

The camera projection is blended azimuthally between the two slice halves, (1 ± x/r)/2, so a swaying
flame has no seam at the axis. Default exposure in `web/app.js` is now `EXPO0 = 1.7` at 0 EV (was 4.2).

**Dance (2026-10-10).** In a still room the flame is perfectly steady (laminar, as it should be), so
the motion comes from three modelled sources: `WICK_CURL = 6` cells, the vapour source and glowing tip
sit to one side as a braided wick curls (the page draws the curve); `SUPPLY_SIGMA = 0.12`, `SUPPLY_TAU
= 0.3 s`, an Ornstein–Uhlenbeck noise on the vapour supply (capillary feed and melt pool), which gives
the height pulsing and brightness flicker; and the room-air model `DRAFT_SIGMA = 60` mm/s, `DRAFT_TAU =
0.8 s`, `DRAFT_COUPLING = 3` /s (was 40, 0.8, 0.8: the window's air lagged the wind by 1.25 s, which
filtered most of it out). Measured with `FLICKER=1 node tools/snapshot.js`: sway σ 0.7 mm, bob σ 0.6 mm,
brightness σ 4 % at draft 1x; sway σ 2.5 mm at 3x. Vorticity confinement does nothing for the dance.
Knobs: `fl_set_wick(curl, supply_sigma)`, `fl_set_draft_model(sigma, tau, coupling)`.

**Rendering (2026-10-10).** The page presents the engine's linear-light field on the GPU
(`makePresenter` in `web/app.js`, WebGL2): Catmull-Rom over the 168 x 280 field per device pixel, a
bloom from the engine's 1/4-resolution Gaussian (`fl_bloom`, 1.5 % of linear light: veiling glare),
then ACES and sRGB in the shader. `fl_set_output(1)` makes the engine skip its own 2x upsample and
tone table (shading 1.4 -> 0.5 ms); the canvas 2D path with `fl_pixels` stays as the fallback.
Dithering is disabled (screen-door pattern otherwise). The old "room glow" was a radial gradient
falling off within ~8 mm of the flame and read as haze; it is now a wall 12 cm behind the candle lit
by cos(theta)/d^2, nearly flat across the window. Default exposure `EXPO0 = 1.4`. The luminous edge
is 0.5 to 0.75 mm wide in the engine itself (sheath thickness at this grid); soot burnout activation
(`fl_set_oxidation`) does not sharpen it. Soot carries ~20 mm above the luminous tip in the soot view:
the model candle smokes faintly; a real one in still air closes its tip. Open physics item.
`window.candleDebug.setBloom(k)` is a console hook for tuning.

**Projection on a bent flame (2026-10-10).** The camera's line-of-sight projection was about the
wick column; when the draft bent the flame off it, one half projected into a boxy fill and the flame
looked split in two. Each row is now projected about its own centre of light (`render`).

**The lobes: numerical, fixed by the substep (2026-10-10).** The stacked bright lobes were soot bands
along the core, T and S in antiphase, period ~11 cells. `examples/axis.rs` (now a full probe along
the flame's own centreline, all knobs via env) showed the band period halves when the substep halves
and the amplitude goes 0.034 -> 0.028 -> 0.026 -> 0.006 at 1/120, 1/160, 1/180, 1/240 s: a
splitting oscillation between the transport step and the chemistry, locked to the step. Chemistry
sub-cycling alone reaches 0.013, diffusion sub-cycling and Strang splitting do nothing, so
`SUB_DT = 1/240` (converged; 1/300 is the same). Cost: physics 11.5 ms per frame at 60 fps in Chrome
(was 6). Consequences, all recalibrated: the flame is brighter and hotter (less numerical diffusion):
`FUEL 0.7` restores 18.5 mm; the 4 m/s gust no longer extinguishes (at 1/120 it moved 133 cells per
step and swept the flame out of the window numerically), the page blows at 5 m/s; the room wind
reaches the flame about half as much (`examples/wind.rs`: 35 vs 21 mm/s at the flame for the same
57 mm/s room wind, the resolved hot plume shields itself), so `DRAFT_SIGMA 130` for the same dance
and the draft slider tops out at 2x. The eddy impulse is now independent of dt (it scaled with it).
The T^4 cooling is integrated exactly over the step; the wick source relaxations are rates.

**Smoke column and its scallops (2026-10-10).** Soot escaped 15 mm above the luminous tip and the
camera painted it as smoke (the `sm` scattering term); the column's edge carried a scalloped pattern
whose period moved with the substep (34, 21, 13 cells at 1/120, 1/240, 1/480): a step-locked artefact
of the first-order semi-Lagrangian back-trace in the sheared plume (not vorticity confinement, not the
pressure solve: `examples/plume.rs` tested both). Advection now uses midpoint-rule departure points
(`rk2`), after which the wavelength converges to ~3 mm, a physical shear-layer scale. The tip now
closes: `SO = 2e6` (was 1e6), smoke 1 mm above the glowing soot instead of 15, radiant 26 %.
Exposure 0.36.

With the midpoint back-trace no gust extinguished the flame (even 8 m/s for 0.8 s): the old blow-out
had been the first-order trace sweeping the flame out of the window in one step, and the model's
chemistry (burning above 600 K) relights the vapour from the hot surface. Extinction is now also a
wick rule: a gust that cools the wick below `WICK_COLD = 750 K` puts the candle out (a 5 m/s puff for
0.4 s cools it to ~680 K, the breeze only to ~990). `FUEL 0.9` for 18 mm with the closed tip.

The draft slider tops out at 1.5x: at 2x the wind blows the candle out within seconds, which is physics but not a setting a slider should land on.

Physics experiments from this session, all behind runtime knobs and tool env vars, none shipped:
two-step soot (inception + surface growth, `fl_set_growth`, `fl_set_inception`) makes a dark core
but a dim flame unless the whole chemistry is recalibrated; a 2 mm glowing tip (`fl_set_wick_model`)
anchors only with kinetics x10 (`fl_set_chem`); vapour cooling has no effect under air-like
conduction; the dense-vapour diffusivity rule (`fl_set_vapour`, physical ~25) cools the core but
grows the flame; entrainment modes (`fl_set_entrain_mode` 1, 2) keep oxygen out of the core but the
core then burns at the base. The committed hot-core structure (a 13 mm fuel core at ~1,400 K with
soot in it) is a tuned compromise, not a candle's dark zone; the honest version needs those pieces
retuned together, and that remains open.

## Where things stand (2026-10-09)

- Working page with: strike (sparks), steady burn, breeze (bends, turns blue), blow out (goes out,
  smoulders, smoke), new match. Camera / Temperature / Soot / Reaction views. Live readings: soot
  temperature, peak wavelength, visible share of radiation, radiant fraction, spectrum chart.
  Controls: exposure, white balance, O₂ %, gravity, room draft. Engine switch with live timings.
- Rust/wasm engine matches the JS engine physically (identical burn rate; soot T within about 15 K,
  since the room draft is random and the JS engine's random numbers are unseeded) and is about 4x
  faster for the physics, 2x for shading.
- `tools/build.py --no-cargo` reproduces the published page byte-for-byte, and the Rust source
  in `engine/` reproduces the embedded wasm byte-for-byte.
- Published as a private claude.ai artifact named "Blackbody Candle" (version 2). That page is
  `public/blackbody-candle/index.html` exactly.

## The model, briefly

Grid 84 x 140 cells, 0.5 mm each (42 x 70 mm). Match head at cell (42, 78). Substeps of 1/120 s,
1 to 3 per frame. Fields: velocity u, v; temperature T; fuel vapour F; oxygen O (1 = room air);
soot S; reaction rate R.

Each substep, in this order (order matters, see "lessons"):

1. **Forces**: buoyancy `g·(T−T∞)/T`, vorticity confinement, room draft (Ornstein–Uhlenbeck random
   wander applied uniformly + occasional small eddies), gust if blowing.
2. **Match head**: an ellipse of source cells injects fuel and holds the gas near the head at the
   ember temperature (the glowing char heats the gas). At strike: an ignition kernel (2300 K for
   0.12 s) and a head "charge" that adds heat and fuel, decaying over ~0.45 s.
3. **Project** (pressure solve, 16 red-black Gauss–Seidel iterations, p reset to 0 each time) with
   a thermal-expansion source `EXPAND·Q·R/T`.
4. **Advect** all six fields along the projected velocity (one fused semi-Lagrangian pass).
5. **Diffuse** T, F, O (and a little u, v), red-black, effective D = 420 mm²/s.
6. **Out-of-plane entrainment**: mixes each cell toward room air at a rate ∝ |velocity| / 10 cells.
   This stands in for the air a round flame draws in from the third dimension. Without it the 2D
   flame is roughly twice too tall and its tip never closes.
7. **Chemistry + radiation**: one-step fuel + O₂ (Arrhenius), soot formed where fuel is hot,
   oxidised by O₂ plus an OH proxy, optically thin radiative loss `KRAD·(S+0.04)·(T⁴−T∞⁴)`.

Match life: heat feedback `h` from the hottest gas touching the head (attached flame) drives
pyrolysis and the ember temperature. Blowing cools the ember (forced convection), the flame
detaches, `h` collapses, phase goes `lit → out`, the char smoulders and smokes for a few seconds.
The match also goes out after burning 40 cells (20 mm) of stick.

Light: per cell, soot XYZ from a table of ∫ B(λ,T)/λ · CMF(λ) dλ (soot emissivity ∝ 1/λ),
plus CH*/C₂*/CO₂* band light ∝ R. Bradford white balance from a blackbody white to D65, XYZ →
linear sRGB, exposure, 2x bilinear upsample in linear light, ACES tone curve + sRGB via a 64K table.
The ember and sparks are opaque grey bodies (∫ B · CMF). CIE curves use the Wyman–Sloan–Shirley
(2013) analytic fit.

## Tuned constants (`engine/src/lib.rs`, mirrored in `web/core.js`)

| Constant | Value | What it does |
| --- | --- | --- |
| `A`, `EA` | 1e5 /s, 6000 K | Reaction rate. Lower A and the flame lifts off and blows out on its own. |
| `STO`, `Q` | 6, 8700 K | O₂ per fuel and heat release. Sets flame width and peak T (~1,800 K gas). |
| `SF`, `ESF` | 1.5e5, 12000 K | Soot formation. Controls how yellow vs blue the flame is. |
| `SO`, `ESO`, `OHP` | 1e6, 11000 K, 0.012 | Soot burnout. Closes the luminous tip. |
| `KRAD` | 5e-9 | Radiative loss. Gives ~18% radiant fraction and ~1,620 K soot. |
| `DIFF` | 420 mm²/s | Effective diffusivity. Width, anchoring. |
| `ENT` | 0.25 | 3D entrainment stand-in. Flame height (~20 mm) and tip closure. |
| `EXPAND` | 0.5 | Thermal expansion in the pressure solve. |
| vorticity `eps` | 24 cells/s | Swirl. |
| fuel scale | 0.3 in `src_strength` | Fuel flow. Flame size. |
| draft | σ 40 mm/s, τ 0.8 s | Room draft. Sway. The UI slider multiplies it. |
| gust cooling | 0.4 per (m/s) per s | How fast blowing cools the char. |
| feedback | `(nearT−900)/600`, ember ≤ 1150 K | Heat feedback and anchoring. |

UI defaults (in `web/app.js`): exposure 4.2 at 0 EV, white balance 2800 K,
blue-zone gain `CHEMI = 0.00022`, breeze 0.35 m/s for 1.5 s, blow 4 m/s for 0.4 s.

**If you change physics, change it in both `lib.rs` and `core.js`** (the JS engine exists for the
A/B switch), or drop the JS engine. Check with `node tools/bench.js`: the two should agree (burned cells identical, soot T within ~15 K).

## Lessons (things that went wrong, so they don't again)

- **Pressure warm start** (reusing last step's p) caused a slow upward-velocity drift; after ~5 s
  the flame lifted off and died. Fixed by resetting p to 0 every solve.
- **Advect scalars with the projected velocity**, not the pre-projection one, or the flame bends
  sideways and downward.
- **The top boundary must block backflow** (v ≤ 0), or a downward jet grows from the outlet.
- **Draft must be a uniform force.** Forcing only part of the domain creates a shear layer that
  tips the flame over.
- **Anchoring needed a hot-surface model** (ember temperature held near the head plus heat
  feedback). Without it, fresh air swept the base away and the flame blew off.
- **The white-balance table must cover up to 7000 K.** It once stopped at 3200 K, so every white
  balance setting above that silently behaved like 3200 K.
- **Blue-zone gain must stay small.** Larger values make the flame and the room glow pink at warm
  white balance, because soot has almost no blue and the camera boosts blue.
- **Performance**:
  - In-order Gauss–Seidel diffusion was latency-bound, because each cell waited on the one just
    written. Red-black ordering made it 9x faster.
  - Six separate advection passes became one fused pass.
  - The per-pixel ACES curve became a table lookup.
  - It was not denormal floats (checked).
- **Toolchain on the cloud VM**: `rustup target add wasm32-unknown-unknown` was blocked there.
  The wasm was built by fetching Rust 1.97.0's `library/` (and the `backtrace` submodule) from
  GitHub into the sysroot as `rust-src`, then
  `RUSTC_BOOTSTRAP=1 cargo build -Zbuild-std=std,panic_abort --target wasm32-unknown-unknown`.
  On a normal laptop, plain `rustup target add` + `cargo build` is all you need.

## Known issues and limits

- **Real-device frame rate is unmeasured.** In headless Chromium (software canvas) the engine
  takes ~5 ms but the whole frame ~25–30 ms, because canvas drawing dominates. On a GPU browser it
  should be far cheaper. First thing to check on the laptop: the Engine readouts.
- Flicker comes mostly from the room draft; the grid is too coarse and diffusive for natural
  buoyant puffing.
- 2D slice with an entrainment stand-in; one-step chemistry; constants tuned by eye toward a
  20 mm tall, 7 mm wide flame. Trends are physical, absolute numbers are not predictions.
- The flame base can look slightly stepped (the head source is a few cells wide).
- The radiant-fraction reading starts high (~44%) during the strike flare, then settles near 18%.
- `cargo clippy` reports 5 minor warnings.
- Fonts come from Google Fonts with system fallbacks.

## Good next steps

1. Measure on the laptop (Chrome, Safari, Firefox) and on a phone. If canvas drawing is the
   bottleneck, move the room glow, stick and bloom into wasm (draw the whole frame into the pixel
   buffer), or into WebGL2.
2. Run the engine in a Web Worker (created from a blob URL) so physics and drawing overlap.
3. Higher resolution (e.g. 0.33 mm cells). Many constants are in cell units (head ellipse, ENT
   radius, poke radius, source sizes) and would need retuning; `tools/snapshot.js` is the quickest
   loop for that.
4. Less numerical diffusion (MacCormack or BFECC advection) for livelier flicker.
5. An axisymmetric (r, z) solver instead of the 2D slice, which would remove the `ENT` stand-in.
6. SIMD for the pressure solve (Jacobi on wide lanes, or a small multigrid).

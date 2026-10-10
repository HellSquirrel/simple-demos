// Headless tuning harness: run the wasm engine and save a strip of frames, no browser needed.
// Prints readings at each snapshot time and writes snapshot.ppm (convert with any image tool,
// e.g. `python3 -c "from PIL import Image; Image.open('snapshot.ppm').save('snapshot.png')"`).
//
// Usage:  node tools/snapshot.js
// Env:    TIMES=0.3,2,4,8     seconds after strike to capture
//         VIEW=0|1|2|3        camera | temperature | soot | reaction
//         EV=0  WB=2800       exposure (EV) and white balance (K)
//         O2=20.9 G=1 DRAFT=1 room
//         GUST_AT=3 GUST_V=4 GUST_DUR=0.4   optional gust (m/s, s)
//         PIT=16              pressure sweeps per substep (all knobs below are optional; unset = engine default)
//         ALPHA=22 NU=15.6    thermal diffusivity and kinematic viscosity at 300 K, mm^2/s (both scale as T^1.7)
//         FUEL=0.5            wax vapour supply of the wick
//         SOOT=1 KRAD=1       multipliers on soot formation and radiative loss
//         AS=1 QS=1           multipliers on the burn pre-exponential and the heat release per unit fuel
//         CURL=6 SUPPLY=0.12  wick curl (cells) and vapour-supply noise; SWIRL=1 vorticity confinement
//         FLICKER=1           after the first snapshot time, sample the light for 6 s and report sway / bob / flicker
//         ESO=11000 KEEP=1700 soot burnout activation temperature; the rate at KEEP kelvin is held fixed
//         DSIG=60 DTAU=0.8 DCPL=3     room-air model: wind sigma (mm/s at draft 1x), correlation time, coupling rate
const fs = require('fs'), path = require('path');
(async () => {
  const bytes = fs.readFileSync(path.join(__dirname, '../engine/target/wasm32-unknown-unknown/release/flame.wasm'));
  const { instance } = await WebAssembly.instantiate(bytes, {});
  const w = instance.exports;
  const env = (k, d) => (process.env[k] !== undefined ? +process.env[k] : d);
  w.fl_init(); w.fl_seed(1234);
  w.fl_set_camera(4.2 * Math.pow(2, env('EV', 0)), env('WB', 2800), 0.00022);
  w.fl_set_room(env('O2', 20.9) / 100, env('G', 1), env('SWIRL', 1), env('DRAFT', 1));
  // tuning knobs: applied only when given, so the engine's own defaults are what runs otherwise
  const E = process.env;
  if (E.PIT !== undefined) w.fl_set_solver(+E.PIT);
  if (E.ALPHA !== undefined || E.NU !== undefined) w.fl_set_diffusion(env('ALPHA', 22), env('NU', 15.6));
  if (E.FUEL !== undefined) w.fl_set_fuel(+E.FUEL);
  if (E.SOOT !== undefined) w.fl_set_soot(+E.SOOT);
  if (E.KRAD !== undefined) w.fl_set_krad(+E.KRAD);
  if (E.AS !== undefined || E.QS !== undefined) w.fl_set_chem(env('AS', 1), env('QS', 1));
  if (E.CURL !== undefined || E.SUPPLY !== undefined) w.fl_set_wick(env('CURL', 6), env('SUPPLY', 0.12));
  if (E.ESO !== undefined) w.fl_set_oxidation(0, +E.ESO, env('KEEP', 1700));
  if (E.DSIG !== undefined || E.DTAU !== undefined || E.DCPL !== undefined) w.fl_set_draft_model(env('DSIG', 60), env('DTAU', 0.8), env('DCPL', 3));
  w.fl_reset(); w.fl_set_view(env('VIEW', 0)); w.fl_strike(0);
  const times = (process.env.TIMES || '0.3,2,4,8').split(',').map(Number);
  const u = w.fl_updims(), UW = u >>> 16, UH = u & 0xffff;
  const gustAt = env('GUST_AT', -1);
  let t = 0, gusted = false; const frames = [];
  // how much does the flame dance? sample the light centroid and total light every frame for 6 s
  const flicker = () => {
    const st = new Float32Array(w.memory.buffer, w.fl_stats(), 24);
    const cx = [], cy = [], yy = [];
    for (let n = 0; n < 360; n++) { w.fl_step(1 / 60); t += 1 / 60; w.fl_render(); cx.push(st[4]); cy.push(st[5]); yy.push(st[1]); }
    const sd = a => { const m = a.reduce((p, q) => p + q, 0) / a.length; return [m, Math.sqrt(a.reduce((p, q) => p + (q - m) ** 2, 0) / a.length)]; };
    const [mx, sx] = sd(cx), [my, sy] = sd(cy), [mY, sY] = sd(yy);
    const range = a => Math.max(...a) - Math.min(...a);
    console.log(`dance over 6 s: sway σ ${(sx * 0.25).toFixed(2)} mm (range ${(range(cx) * 0.25).toFixed(1)}), bob σ ${(sy * 0.25).toFixed(2)} mm (range ${(range(cy) * 0.25).toFixed(1)}), flicker σ ${(100 * sY / mY).toFixed(1)} % (range ${(100 * range(yy) / mY).toFixed(0)} %)`);
  };
  for (const target of times) {
    while (t < target) {
      if (gustAt >= 0 && !gusted && t >= gustAt) { w.fl_blow(env('GUST_V', 4), env('GUST_DUR', 0.4)); gusted = true; }
      w.fl_step(1 / 60); t += 1 / 60;
    }
    if (process.env.FLICKER && target === times[0]) flicker();
    w.fl_render();
    const st = new Float32Array(w.memory.buffer, w.fl_stats(), 24);
    const phase = ['ready', 'lit', 'out'][st[7] | 0];
    console.log(`t=${target.toFixed(2)}s  ${phase}  sootT=${st[3].toFixed(0)}K  Y=${st[1].toFixed(1)}  ember=${st[9].toFixed(0)}K  burned=${st[10].toFixed(1)} cells  radiant=${(st[13] / Math.max(1e-6, st[12]) * 100).toFixed(0)}%`);
    frames.push(Buffer.from(new Uint8Array(w.memory.buffer, w.fl_pixels(), UW * UH * 4)));
  }
  const n = frames.length, Wd = UW * n, out = Buffer.alloc(Wd * UH * 3);
  frames.forEach((f, q) => { for (let y = 0; y < UH; y++) for (let x = 0; x < UW; x++) { const k = (x + y * UW) * 4, o = (q * UW + x + y * Wd) * 3; out[o] = f[k]; out[o + 1] = f[k + 1]; out[o + 2] = f[k + 2]; } });
  fs.writeFileSync('snapshot.ppm', Buffer.concat([Buffer.from(`P6 ${Wd} ${UH} 255\n`), out]));
  console.log(`wrote snapshot.ppm (${n} frames, ${UW}x${UH} each)`);
})();

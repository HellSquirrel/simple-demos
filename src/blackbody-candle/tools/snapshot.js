// Headless harness: run the wasm engine and save a strip of frames, no browser needed. Prints readings at each
// snapshot time and writes snapshot.ppm (168 x 280 per frame; convert with any image tool).
//
// Usage:  node tools/snapshot.js
// Env:    TIMES=0.3,2,4,8     seconds after lighting to capture
//         VIEW=0|1|2|3        camera | temperature | soot | reaction
//         EV=0  WB=2800       exposure (EV about the page default) and white balance (K)
//         O2=20.9 G=1 DRAFT=1 room
//         GUST_AT=3 GUST_V=5 GUST_DUR=0.4   optional gust (m/s, s)
//         FLICKER=1           after the first snapshot time, sample the light for 6 s and report sway / bob / flicker
const fs = require('fs'), path = require('path');
(async () => {
  const bytes = fs.readFileSync(path.join(__dirname, '../engine/target/wasm32-unknown-unknown/release/flame.wasm'));
  const { instance } = await WebAssembly.instantiate(bytes, {});
  const w = instance.exports;
  const env = (k, d) => (process.env[k] !== undefined ? +process.env[k] : d);
  w.fl_init(); w.fl_seed(1234);
  w.fl_set_camera(0.36 * Math.pow(2, env('EV', 0)), env('WB', 2800), 0.00022);
  w.fl_set_room(env('O2', 20.9) / 100, env('G', 1), 1, env('DRAFT', 1));
  w.fl_reset(); w.fl_set_view(env('VIEW', 0)); w.fl_strike();
  const times = (process.env.TIMES || '0.3,2,4,8').split(',').map(Number);
  const d = w.fl_dims(), W = d >>> 16, H = d & 0xffff;
  const gustAt = env('GUST_AT', -1);
  let t = 0, gusted = false; const frames = [];
  const stats = () => new Float32Array(w.memory.buffer, w.fl_stats(), 16);
  const flicker = () => {
    const cx = [], cy = [], yy = [];
    for (let n = 0; n < 360; n++) { w.fl_step(1 / 60); t += 1 / 60; w.fl_render(); const st = stats(); cx.push(st[4]); cy.push(st[5]); yy.push(st[1]); }
    const sd = a => { const m = a.reduce((p, q) => p + q, 0) / a.length; return [m, Math.sqrt(a.reduce((p, q) => p + (q - m) ** 2, 0) / a.length)]; };
    const [, sx] = sd(cx), [, sy] = sd(cy), [mY, sY] = sd(yy);
    const range = a => Math.max(...a) - Math.min(...a);
    console.log(`dance over 6 s: sway σ ${(sx * 0.25).toFixed(2)} mm (range ${(range(cx) * 0.25).toFixed(1)}), bob σ ${(sy * 0.25).toFixed(2)} mm (range ${(range(cy) * 0.25).toFixed(1)}), flicker σ ${(100 * sY / mY).toFixed(1)} % (range ${(100 * range(yy) / mY).toFixed(0)} %)`);
  };
  // the engine gives linear light; tone-map it here the way the page's shader does
  const aces = x => { x = Math.max(0, x); return Math.min(1, (x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14)); };
  const srgb = c => Math.round(255 * (c <= 0.0031308 ? 12.92 * c : 1.055 * Math.pow(c, 1 / 2.4) - 0.055));
  for (const target of times) {
    while (t < target) {
      if (gustAt >= 0 && !gusted && t >= gustAt) { w.fl_blow(env('GUST_V', 5), env('GUST_DUR', 0.4)); gusted = true; }
      w.fl_step(1 / 60); t += 1 / 60;
    }
    if (process.env.FLICKER && target === times[0]) flicker();
    w.fl_render();
    const st = stats();
    const phase = ['ready', 'lit', 'out'][st[7] | 0];
    console.log(`t=${target.toFixed(2)}s  ${phase}  sootT=${st[3].toFixed(0)}K  Y=${st[1].toFixed(1)}  ember=${st[9].toFixed(0)}K  radiant=${(st[12] / Math.max(1e-6, st[11]) * 100).toFixed(0)}%`);
    const lin = new Float32Array(w.memory.buffer, w.fl_linear(), W * H * 3), tone = env('VIEW', 0) === 0;
    const px = Buffer.alloc(W * H * 3);
    for (let i = 0; i < W * H * 3; i++) px[i] = tone ? srgb(aces(lin[i])) : Math.round(255 * Math.min(1, Math.max(0, lin[i])));
    frames.push(px);
  }
  const n = frames.length, Wd = W * n, out = Buffer.alloc(Wd * H * 3);
  frames.forEach((f, q) => { for (let y = 0; y < H; y++) f.copy(out, (q * W + y * Wd) * 3, y * W * 3, (y + 1) * W * 3); });
  fs.writeFileSync('snapshot.ppm', Buffer.concat([Buffer.from(`P6 ${Wd} ${H} 255\n`), out]));
  console.log(`wrote snapshot.ppm (${n} frames, ${W}x${H} each)`);
})();

// Node benchmark of the wasm engine: light the candle, 10 s of burning, per-frame timings and readings.
// Usage: node tools/bench.js   (build the engine first)
const fs = require('fs');
const path = require('path');
const { performance } = require('perf_hooks');
(async () => {
  const bytes = fs.readFileSync(path.join(__dirname, '../engine/target/wasm32-unknown-unknown/release/flame.wasm'));
  const mod = await WebAssembly.compile(bytes);
  console.log('imports:', WebAssembly.Module.imports(mod).map(i => i.module + '.' + i.name));
  const { exports: w } = await WebAssembly.instantiate(mod, {});
  w.fl_init(); w.fl_seed(12345); w.fl_strike(0);
  const F = 600;
  let ts = 0, tr = 0;
  for (let n = 0; n < F; n++) { const a = performance.now(); w.fl_step(1 / 60); const b = performance.now(); w.fl_render(); const c = performance.now(); if (n > 60) { ts += b - a; tr += c - b; } }
  const st = new Float32Array(w.memory.buffer, w.fl_stats(), 24);
  const phase = ['ready', 'lit', 'out'][st[7] | 0];
  console.log(`wasm: sim ${(ts / (F - 61)).toFixed(2)} ms  render ${(tr / (F - 61)).toFixed(2)} ms   phase ${phase}  sootT ${st[3].toFixed(0)} K  Y ${st[1].toFixed(1)}  radiant ${(st[13] / Math.max(1e-6, st[12]) * 100).toFixed(0)}%`);
})();

/* The Rust engine, compiled to WebAssembly, behind one small interface over its C ABI. */
const PHASES = ['ready', 'lit', 'out'];
const VIEW_CODE = { camera: 0, temp: 1, soot: 2, heat: 3 };

async function makeEngine(b64) {
  const bin = atob(b64), bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  const { instance } = await WebAssembly.instantiate(bytes, {});
  const w = instance.exports;
  w.fl_init(); w.fl_seed((Math.random() * 4294967296) >>> 0);
  const d = w.fl_dims(), W = d >>> 16, H = d & 0xffff, bd = w.fl_bloomdims(), BW = bd >>> 16, BH = bd & 0xffff;
  let buf = null, S, M, SC, LIN, BLOOM;
  const views = () => { // wasm memory can grow, which detaches old views
    if (buf === w.memory.buffer) return;
    buf = w.memory.buffer;
    S = new Float32Array(buf, w.fl_stats(), 16);
    M = new Float32Array(buf, w.fl_matrix(), 9);
    SC = new Float32Array(buf, w.fl_scratch(), 4);
    LIN = new Float32Array(buf, w.fl_linear(), W * H * 3);
    BLOOM = new Float32Array(buf, w.fl_bloom(), BW * BH * 3);
  };
  views();
  const stats = {};
  const sync = () => {
    views();
    stats.X = S[0]; stats.Y = S[1]; stats.Z = S[2]; stats.sootT = S[3]; stats.cx = S[4]; stats.cy = S[5];
    stats.headY = S[6]; stats.phase = PHASES[S[7] | 0]; stats.h = S[8]; stats.ember = S[9]; stats.since = S[10];
    stats.qrel = S[11]; stats.qrad = S[12];
  };
  const eng = {
    W, H, BW, BH, headX: w.fl_head_x(), stats,
    step: dt => w.fl_step(dt),
    render: () => { w.fl_render(); sync(); },
    strike: () => { const r = w.fl_strike(); w.fl_render(); sync(); return r; },
    reset: () => { w.fl_reset(); w.fl_render(); sync(); },
    blow: (s, t) => w.fl_blow(s, t),
    poke: (x, y, vx, vy) => w.fl_poke(x, y, vx, vy),
    setRoom: (o2, g, draft) => w.fl_set_room(o2, g, 1, draft),
    setCamera: (expo, wb, chemi) => { w.fl_set_camera(expo, wb, chemi); views(); },
    setView: v => w.fl_set_view(VIEW_CODE[v]),
    linear: () => { views(); return LIN; },  // W x H x RGB, linear light after exposure (camera view) or 0..1 map values
    bloom: () => { views(); return BLOOM; }, // BW x BH x RGB, Gaussian of the camera image at 1/4 resolution
    greyRgb: (T, gain) => { w.fl_grey_rgb(T, gain); views(); return [SC[0], SC[1], SC[2]]; },
    matrix: () => { views(); return M; },
    visibleShare: T => w.fl_visible_share(T),
  };
  eng.render();
  return eng;
}

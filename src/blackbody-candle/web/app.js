(async () => {
  const calm = matchMedia('(prefers-reduced-motion: reduce)').matches;
  const $ = id => document.getElementById(id);

  // ---------- engine ----------
  let eng;
  try { eng = await makeWasmEngine(WASM_B64); }
  catch (e) { $('engNote').textContent = `WebAssembly failed to load here (${e && e.message || e}), so there is no flame.`; return; }
  const W = eng.W, H = eng.H, UW = eng.UW, UH = eng.UH, HX = eng.headX;
  let BLOOM = 0.015; // a camera's veiling glare: this share of the 1.5 mm Gaussian of the linear image is added back
  const WALL = 480; // a wall this many cells (12 cm) behind the candle is what the flame lights; irradiance goes as cos(theta) / d^2
  window.candleDebug = { setBloom: k => { BLOOM = k; }, presenter: () => !!glx }; // for tuning and checks from the console

  // ---------- tone curve for colours drawn by the canvas API (glow, candle, sparks) ----------
  const ACES = x => { x = x < 0 ? 0 : x; const y = (x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14); return y > 1 ? 1 : y; };
  const GAM = new Uint8ClampedArray(4097);
  for (let i = 0; i <= 4096; i++) { const c = i / 4096; GAM[i] = 255 * (c <= 0.0031308 ? 12.92 * c : 1.055 * Math.pow(c, 1 / 2.4) - 0.055); }
  const enc = x => GAM[(ACES(x) * 4096) | 0];
  const rgb = (l, a = 1) => `rgba(${enc(l[0])},${enc(l[1])},${enc(l[2])},${a})`;

  // ---------- camera and room settings ----------
  const CHEMI = 0.00022;               // CH*/C2* light per unit reaction rate, relative to soot
  const EXPO0 = 0.36; // camera exposure at 0 EV, set so the core is bright but not a flat clip
  const cam = { expo: EXPO0, wb: 2800 }, room = { o2: 0.209, g: 1, draft: calm ? 0 : 1 };
  let view = 'camera';
  eng.setCamera(cam.expo, cam.wb, CHEMI); eng.setRoom(room.o2, room.g, room.draft); eng.setView(view);

  // ---------- canvases ----------
  const cv = $('flame'), ctx = cv.getContext('2d');
  const off = document.createElement('canvas'); off.width = UW; off.height = UH;
  const octx = off.getContext('2d');
  const blur = document.createElement('canvas'); blur.width = Math.ceil(W / 8); blur.height = Math.ceil(H / 8); // bloom layer at 2 mm per pixel
  const bctx = blur.getContext('2d');
  let cw = 0, ch = 0, s = 1;
  function resize() {
    const r = cv.getBoundingClientRect(), d = Math.min(2, devicePixelRatio || 1); // full resolution up to 2x; a 3x phone gains nothing visible
    cw = cv.width = Math.round(r.width * d); ch = cv.height = Math.round(r.height * d); s = cw / W;
    if (glx) glx.resize(cw, ch);
  }

  // ---------- GPU presenter: the engine's linear-light field, resampled and tone-mapped per device pixel ----------
  // The engine computes light per cell. Presenting it at display resolution is a resampling job, so it goes to the GPU:
  // Catmull-Rom over the 168 x 280 field, a small bloom from the engine's 1/4-resolution Gaussian, then ACES and sRGB.
  // Without WebGL2 the engine's own 2x 8-bit output is drawn through the canvas instead.
  function makePresenter() {
    const cvs = document.createElement('canvas');
    const gl = cvs.getContext('webgl2', { alpha: false, antialias: false, depth: false, premultipliedAlpha: false });
    if (!gl) return null;
    const vs = `#version 300 es
in vec2 a; out vec2 v;
void main() { v = vec2(a.x * .5 + .5, .5 - a.y * .5); gl_Position = vec4(a, 0., 1.); }`;
    const fs = `#version 300 es
precision highp float;
uniform sampler2D uLin, uBloom; uniform vec2 uSrc, uBloomSize; uniform float uTone, uBloomK;
in vec2 v; out vec4 o;
vec3 fetchL(ivec2 p) { return texelFetch(uLin, clamp(p, ivec2(0), ivec2(uSrc) - 1), 0).rgb; }
vec4 cr(float t) { float t2 = t * t, t3 = t2 * t; return vec4(-t3 + 2. * t2 - t, 3. * t3 - 5. * t2 + 2., -3. * t3 + 4. * t2 + t, t3 - t2) * .5; }
vec3 bicubic(vec2 uv) {
  vec2 p = uv * uSrc - .5; vec2 f = fract(p); ivec2 i = ivec2(floor(p)); vec4 wx = cr(f.x), wy = cr(f.y); vec3 s = vec3(0.);
  for (int y = -1; y <= 2; y++) { vec3 row = vec3(0.); for (int x = -1; x <= 2; x++) row += wx[x + 1] * fetchL(i + ivec2(x, y)); s += wy[y + 1] * row; }
  return max(s, 0.);
}
vec3 bloomAt(vec2 uv) {
  vec2 p = uv * uBloomSize - .5; vec2 f = fract(p); ivec2 i = ivec2(floor(p)); ivec2 m = ivec2(uBloomSize) - 1;
  vec3 a = texelFetch(uBloom, clamp(i, ivec2(0), m), 0).rgb, b = texelFetch(uBloom, clamp(i + ivec2(1, 0), ivec2(0), m), 0).rgb;
  vec3 c = texelFetch(uBloom, clamp(i + ivec2(0, 1), ivec2(0), m), 0).rgb, d = texelFetch(uBloom, clamp(i + ivec2(1, 1), ivec2(0), m), 0).rgb;
  return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}
vec3 aces(vec3 x) { return clamp((x * (2.51 * x + .03)) / (x * (2.43 * x + .59) + .14), 0., 1.); }
vec3 srgb(vec3 c) { return mix(c * 12.92, 1.055 * pow(c, vec3(1. / 2.4)) - .055, step(.0031308, c)); }
void main() { vec3 c = bicubic(v) + uBloomK * bloomAt(v); if (uTone > .5) c = srgb(aces(c)); else c = clamp(c, 0., 1.); o = vec4(c, 1.); }`;
    const compile = (type, src) => { const sh = gl.createShader(type); gl.shaderSource(sh, src); gl.compileShader(sh); if (!gl.getShaderParameter(sh, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(sh)); return sh; };
    const prog = gl.createProgram(); gl.attachShader(prog, compile(gl.VERTEX_SHADER, vs)); gl.attachShader(prog, compile(gl.FRAGMENT_SHADER, fs)); gl.linkProgram(prog);
    if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(prog));
    gl.useProgram(prog);
    gl.disable(gl.DITHER); // the 8-bit drawing buffer would otherwise get a screen-door pattern
    const buf = gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER, buf); gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 3, -1, -1, 3]), gl.STATIC_DRAW);
    const loc = gl.getAttribLocation(prog, 'a'); gl.enableVertexAttribArray(loc); gl.vertexAttribPointer(loc, 2, gl.FLOAT, false, 0, 0);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
    const tex = (unit, w, h) => {
      const t = gl.createTexture(); gl.activeTexture(gl.TEXTURE0 + unit); gl.bindTexture(gl.TEXTURE_2D, t);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGB32F, w, h, 0, gl.RGB, gl.FLOAT, null);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST); gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE); gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
      return t;
    };
    const tLin = tex(0, W, H), tBloom = tex(1, eng.BW, eng.BH);
    const u = n => gl.getUniformLocation(prog, n);
    gl.uniform1i(u('uLin'), 0); gl.uniform1i(u('uBloom'), 1); gl.uniform2f(u('uSrc'), W, H); gl.uniform2f(u('uBloomSize'), eng.BW, eng.BH);
    const uTone = u('uTone'), uBloomK = u('uBloomK');
    if (gl.getError() !== gl.NO_ERROR) throw new Error('WebGL setup failed');
    return {
      canvas: cvs,
      resize(w, h) { cvs.width = w; cvs.height = h; gl.viewport(0, 0, w, h); },
      present(tone) {
        gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D, tLin); gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, W, H, gl.RGB, gl.FLOAT, eng.linear());
        gl.activeTexture(gl.TEXTURE1); gl.bindTexture(gl.TEXTURE_2D, tBloom); gl.texSubImage2D(gl.TEXTURE_2D, 0, 0, 0, eng.BW, eng.BH, gl.RGB, gl.FLOAT, eng.bloom());
        gl.uniform1f(uTone, tone ? 1 : 0); gl.uniform1f(uBloomK, tone ? BLOOM : 0);
        gl.drawArrays(gl.TRIANGLES, 0, 3);
      },
    };
  }
  let glx = null;
  try { glx = makePresenter(); } catch (e) { console.warn('GPU presenter failed, using the canvas path:', e); glx = null; }
  if (glx) eng.setOutput(1); else console.warn('WebGL2 unavailable, using the canvas path');
  new ResizeObserver(resize).observe(cv); resize();
  if (glx) glx.resize(cw, ch);

  // light from the flame, as linear sRGB, at a point (cell coords)
  function flameLight(x, y, gain) {
    const st = eng.stats, M = eng.matrix();
    const d2 = (x - st.cx) ** 2 + (y - st.cy) ** 2, fall = gain / (1 + d2 / 880); // 880 cells^2 = 55 mm^2
    return [(M[0] * st.X + M[1] * st.Y + M[2] * st.Z) * fall, (M[3] * st.X + M[4] * st.Y + M[5] * st.Z) * fall, (M[6] * st.X + M[7] * st.Y + M[8] * st.Z) * fall];
  }

  function draw() {
    const room = view === 'camera', st = eng.stats, expo = cam.expo;
    if (!glx) octx.putImageData(new ImageData(eng.pixels(), UW, UH), 0, 0);
    ctx.globalCompositeOperation = 'source-over';
    ctx.fillStyle = room ? '#06090e' : '#000'; ctx.fillRect(0, 0, cw, ch);
    if (room && st.Y > 0.05) { // the wall behind the candle, lit by the flame: E(r) = E0 · (WALL² / (WALL² + r²))^1.5, r in the wall plane
      const R = ch * 1.3, g = ctx.createRadialGradient(st.cx * s, st.cy * s, 0, st.cx * s, st.cy * s, R);
      const l0 = flameLight(st.cx, st.cy, 0.0022 * expo);
      for (const f of [0, 0.25, 0.5, 0.75, 1]) { const d = f * R / s, e = Math.pow(WALL * WALL / (WALL * WALL + d * d), 1.5); g.addColorStop(f, rgb(l0.map(v => v * e), 1)); }
      ctx.globalCompositeOperation = 'lighter'; ctx.fillStyle = g; ctx.fillRect(0, 0, cw, ch);
      ctx.globalCompositeOperation = 'source-over';
    }
    drawCandle(room);
    // the flame is emissive and optically thin, so it adds
    ctx.globalCompositeOperation = 'lighter';
    if (glx) {
      glx.present(room); // resampled and tone-mapped at device resolution: a 1:1 copy, no smoothing in between
      ctx.drawImage(glx.canvas, 0, 0);
    } else {
      ctx.imageSmoothingEnabled = true; ctx.imageSmoothingQuality = 'high';
      ctx.drawImage(off, 0, 0, cw, ch);
      if (room) { bctx.clearRect(0, 0, blur.width, blur.height); bctx.drawImage(off, 0, 0, blur.width, blur.height); ctx.globalAlpha = 0.35; ctx.drawImage(blur, 0, 0, cw, ch); ctx.globalAlpha = 1; }
    }
    drawSparks();
    ctx.globalCompositeOperation = 'source-over';
    drawScale();
  }

  // the candle: a wax column with a wick; the wick tip glows as an opaque grey body at the ember temperature
  const WAX_TOP = 18, WAX_HALF = 20, WICK_CURL = 6; // cells; mirror WAX_TOP, WAX_HALF and WICK_CURL in the engine
  function drawCandle(room) {
    const st = eng.stats, expo = cam.expo;
    const hy = st.headY, top = (hy + WAX_TOP) * s, x0 = (HX - WAX_HALF) * s, ww = 2 * WAX_HALF * s;
    const amb = room ? 0.02 : 0.05;
    // wax: ivory, lit by the flame above, fading down the column
    const g = ctx.createLinearGradient(0, top, 0, ch);
    for (const t of [0, 0.08, 0.2, 0.45, 1]) {
      const y = hy + WAX_TOP + t * (H - hy - WAX_TOP), l = room ? flameLight(HX, y, 0.05 * expo) : [0, 0, 0];
      g.addColorStop(t, rgb([0.92 * (l[0] + amb), 0.86 * (l[1] + amb), 0.72 * (l[2] + amb)]));
    }
    ctx.fillStyle = g; ctx.fillRect(x0, top, ww, ch - top);
    // cylinder shading: darker toward the edges
    const sh = ctx.createLinearGradient(x0, 0, x0 + ww, 0);
    sh.addColorStop(0, 'rgba(0,0,0,.45)'); sh.addColorStop(0.3, 'rgba(0,0,0,0)'); sh.addColorStop(0.7, 'rgba(0,0,0,0)'); sh.addColorStop(1, 'rgba(0,0,0,.5)');
    ctx.fillStyle = sh; ctx.fillRect(x0, top, ww, ch - top);
    // the rim: a shallow melt pool
    ctx.fillStyle = 'rgba(0,0,0,.25)';
    ctx.beginPath(); ctx.ellipse(HX * s, top, WAX_HALF * s, 3.2 * s, 0, 0, Math.PI * 2); ctx.fill();
    // wax is translucent: the top of the column glows with the flame's light
    if (room && st.ember > 650) {
      const l = flameLight(HX, hy + WAX_TOP, 0.12 * expo);
      const wg = ctx.createRadialGradient(HX * s, top, 0, HX * s, top, 28 * s);
      wg.addColorStop(0, rgb(l, 0.9)); wg.addColorStop(1, 'rgba(0,0,0,0)');
      ctx.save(); ctx.beginPath(); ctx.rect(x0, top, ww, ch - top); ctx.clip();
      ctx.globalCompositeOperation = 'lighter'; ctx.fillStyle = wg; ctx.fillRect(x0, top, ww, 32 * s);
      ctx.restore();
    }
    // wick: charred cotton, 1 mm wide, rising out of the wax and curling toward the side the engine feeds from (WICK_CURL)
    const tipX = (HX + WICK_CURL) * s, tipY = (hy - 1) * s;
    ctx.strokeStyle = '#0d0907'; ctx.lineWidth = 4 * s; ctx.lineCap = 'round';
    ctx.beginPath(); ctx.moveTo(HX * s, top + 2 * s);
    ctx.quadraticCurveTo(HX * s, (hy + 3) * s, tipX, tipY); ctx.stroke();
    if (st.ember > 650) { // glowing tip
      const l = eng.greyRgb(st.ember, 250 * expo);
      const eg = ctx.createRadialGradient(tipX, tipY, 0, tipX, tipY, 6 * s);
      eg.addColorStop(0, rgb(l, 1)); eg.addColorStop(0.5, rgb(l.map(v => v * 0.4), 0.8)); eg.addColorStop(1, 'rgba(0,0,0,0)');
      ctx.globalCompositeOperation = 'lighter'; ctx.fillStyle = eg;
      ctx.fillRect(tipX - 8 * s, tipY - 8 * s, 16 * s, 16 * s);
      ctx.globalCompositeOperation = 'source-over';
    }
  }

  function drawSparks() {
    ctx.lineCap = 'round';
    for (const [x, y, px, py, T] of eng.sparks()) {
      ctx.strokeStyle = rgb(eng.greyRgb(T, 6 * cam.expo), 1); ctx.lineWidth = Math.max(1, s * 0.45);
      ctx.beginPath(); ctx.moveTo(px * s, py * s); ctx.lineTo(x * s, y * s); ctx.stroke();
    }
  }

  const INF = [[0, 0, 4], [40, 11, 84], [101, 21, 110], [159, 42, 99], [212, 72, 66], [245, 125, 21], [250, 193, 39], [252, 255, 164]];
  function drawScale() {
    const d = cw / 440, x = 14 * d, y = ch - 40 * d, len = 40 * s; // 40 cells = 10 mm
    ctx.strokeStyle = 'rgba(226,230,238,.55)'; ctx.lineWidth = Math.max(1, d);
    ctx.beginPath(); ctx.moveTo(x, y - 4 * d); ctx.lineTo(x, y); ctx.lineTo(x + len, y); ctx.lineTo(x + len, y - 4 * d); ctx.stroke();
    ctx.fillStyle = 'rgba(226,230,238,.7)'; ctx.font = `${11 * d}px "IBM Plex Mono", monospace`;
    ctx.fillText('10 mm', x, y - 8 * d);
    if (view === 'temp') {
      const bx = cw - 26 * d, by = 48 * d, bh = ch * 0.4;
      for (let i = 0; i < 64; i++) {
        const t = (1 - i / 63) * 7, k = Math.min(6, t | 0), a = t - k, c = INF[k].map((v, n) => (v + (INF[k + 1][n] - v) * a) | 0);
        ctx.fillStyle = `rgb(${c[0]},${c[1]},${c[2]})`; ctx.fillRect(bx, by + i * bh / 64, 10 * d, bh / 64 + 1);
      }
      ctx.textAlign = 'right'; ctx.fillText('2200 K', bx - 6 * d, by + 9 * d); ctx.fillText('300 K', bx - 6 * d, by + bh); ctx.textAlign = 'left';
    }
  }

  // ---------- spectrum chart (small JS Planck + CIE fit; it only draws curves) ----------
  const lobe = (x, m, s1, s2) => { const t = (x - m) / (x < m ? s1 : s2); return Math.exp(-0.5 * t * t); };
  const cmf = l => [1.056 * lobe(l, 599.8, 37.9, 31.0) + 0.362 * lobe(l, 442.0, 16.0, 26.7) - 0.065 * lobe(l, 501.1, 20.4, 26.2),
    0.821 * lobe(l, 568.8, 46.9, 40.5) + 0.286 * lobe(l, 530.9, 16.3, 31.1), 1.217 * lobe(l, 437.0, 11.8, 36.0) + 0.681 * lobe(l, 459.0, 26.0, 13.8)];
  const planck = (l, T) => { const x = 1.4387769e7 / (l * T); return x > 700 ? 0 : 1e20 / (Math.pow(l, 5) * Math.expm1(x)); };
  const SRGB = [3.2406, -1.5372, -0.4986, -0.9689, 1.8758, 0.0415, 0.0557, -0.2040, 1.0570];
  const sc = $('spec'), sctx = sc.getContext('2d');
  const band = [];
  for (let l = 380; l <= 780; l += 4) {
    const c = cmf(l);
    let r = SRGB[0] * c[0] + SRGB[1] * c[1] + SRGB[2] * c[2], g = SRGB[3] * c[0] + SRGB[4] * c[1] + SRGB[5] * c[2], b = SRGB[6] * c[0] + SRGB[7] * c[1] + SRGB[8] * c[2];
    const mx = Math.max(r, g, b, 1e-6); r = Math.max(0, r / mx); g = Math.max(0, g / mx); b = Math.max(0, b / mx);
    const e = l < 420 ? (l - 380) / 40 : l > 700 ? (780 - l) / 80 : 1;
    band.push([l, `rgba(${GAM[(r * 4096) | 0]},${GAM[(g * 4096) | 0]},${GAM[(b * 4096) | 0]},${0.25 + 0.55 * e})`]);
  }
  function drawSpectrum(T) {
    const r = sc.getBoundingClientRect(), d = Math.min(2, devicePixelRatio || 1);
    if (sc.width !== Math.round(r.width * d)) { sc.width = Math.round(r.width * d); sc.height = Math.round(r.height * d); }
    const w = sc.width, h = sc.height, padL = 6 * d, padR = 8 * d, padT = 14 * d, padB = 22 * d;
    const L0 = 300, L1 = 3200, X = l => padL + (l - L0) / (L1 - L0) * (w - padL - padR), Y = v => h - padB - v * (h - padT - padB);
    sctx.clearRect(0, 0, w, h);
    for (let i = 0; i < band.length - 1; i++) { sctx.fillStyle = band[i][1]; sctx.fillRect(X(band[i][0]), padT, X(band[i + 1][0]) - X(band[i][0]) + 0.5, h - padT - padB); }
    sctx.fillStyle = '#8a94a8'; sctx.font = `${10.5 * d}px "IBM Plex Mono", monospace`;
    sctx.fillText('visible', X(390), padT - 3 * d);
    sctx.strokeStyle = '#253047'; sctx.lineWidth = d; sctx.beginPath(); sctx.moveTo(padL, h - padB + 0.5); sctx.lineTo(w - padR, h - padB + 0.5); sctx.stroke();
    sctx.textAlign = 'center';
    for (const um of [0.5, 1, 1.5, 2, 2.5, 3]) { const x = X(um * 1000); sctx.fillRect(x, h - padB, d, 4 * d); sctx.fillText(um + (um === 3 ? ' µm' : ''), x, h - 6 * d); }
    sctx.textAlign = 'left';
    if (!(T > 800)) { sctx.fillText('light the match to see its spectrum', X(1150), padT + 30 * d); return; }
    const soot = [], bb = []; let ms = 0, mb = 0;
    for (let l = L0; l <= L1; l += 10) { const b = planck(l, T); bb.push(b); soot.push(b / l); ms = Math.max(ms, b / l); mb = Math.max(mb, b); }
    sctx.beginPath(); sctx.moveTo(X(L0), Y(0));
    soot.forEach((v, i) => sctx.lineTo(X(L0 + i * 10), Y(v / ms)));
    sctx.lineTo(X(L1), Y(0)); sctx.closePath(); sctx.fillStyle = 'rgba(240,180,92,.18)'; sctx.fill();
    sctx.beginPath(); soot.forEach((v, i) => (i ? sctx.lineTo : sctx.moveTo).call(sctx, X(L0 + i * 10), Y(v / ms)));
    sctx.strokeStyle = '#f0b45c'; sctx.lineWidth = 2 * d; sctx.stroke();
    sctx.beginPath(); bb.forEach((v, i) => (i ? sctx.lineTo : sctx.moveTo).call(sctx, X(L0 + i * 10), Y(v / mb)));
    sctx.setLineDash([4 * d, 4 * d]); sctx.strokeStyle = '#8a94a8'; sctx.lineWidth = 1.5 * d; sctx.stroke(); sctx.setLineDash([]);
    const lp = 2404000 / T; // soot peak, nm
    sctx.fillStyle = '#f0b45c'; sctx.beginPath(); sctx.arc(X(lp), Y(1), 3.5 * d, 0, 7); sctx.fill();
    sctx.fillStyle = '#e2e6ee'; sctx.fillText(`${Math.round(T)} K`, Math.min(X(lp) + 8 * d, w - 60 * d), Y(1) + 4 * d);
  }

  // ---------- readouts ----------
  const visCache = new Map();
  const visShare = T => { const k = Math.round(T / 10) * 10; if (!visCache.has(k)) visCache.set(k, eng.visibleShare(k)); return visCache.get(k); };
  let shownT = 0;
  const perf = { sim: 0, shade: 0, draw: 0, frame: 0 };
  function readouts() {
    const st = eng.stats, T = st.sootT, on = T > 800 && st.Y > 0.2;
    shownT = on ? (shownT ? shownT * 0.6 + T * 0.4 : T) : 0;
    $('roT').innerHTML = on ? `${Math.round(shownT).toLocaleString('en-GB')}<small>K</small>` : '–<small>K</small>';
    $('roPeak').innerHTML = on ? `${(2404 / shownT).toFixed(2)}<small>µm</small>` : '–<small>µm</small>';
    const vs = on ? visShare(shownT) * 100 : 0;
    $('roVis').innerHTML = on ? `${vs < 1 ? vs.toFixed(2) : vs.toFixed(1)}<small>%</small>` : '–<small>%</small>';
    const rf = st.qrel > 1e-3 ? st.qrad / st.qrel * 100 : 0;
    $('roRad').innerHTML = st.phase === 'lit' && rf > 0 ? `${rf.toFixed(0)}<small>%</small>` : '–<small>%</small>';
    drawSpectrum(shownT);
    const ph = st.phase, label = ph === 'ready' ? 'Ready' : ph === 'lit' ? `Burning · ${st.since.toFixed(0)} s` : (st.since < 5 ? 'Out · smouldering' : 'Out');
    $('status').dataset.s = ph; $('statusText').textContent = label;
    $('strike').textContent = ph === 'ready' ? 'Light' : 'New candle';
    $('strike').disabled = ph === 'lit';
    $('breeze').disabled = $('blow').disabled = ph !== 'lit';
    $('hint').style.opacity = ph === 'ready' ? 1 : 0;
    $('pfSim').innerHTML = `${perf.sim.toFixed(2)}<small>ms</small>`;
    $('pfShade').innerHTML = `${perf.shade.toFixed(2)}<small>ms</small>`;
    $('pfFrame').innerHTML = `${perf.frame.toFixed(1)}<small>ms</small>`;
    $('pfFps').innerHTML = `${perf.frame > 0 ? Math.min(999, 1000 / perf.frame).toFixed(0) : '–'}<small>fps</small>`;
  }

  // ---------- interaction ----------
  function strikeOrReset() {
    if (eng.stats.phase === 'ready') eng.strike(false); // a lighter, not a match: no sparks
    else if (eng.stats.phase === 'out') eng.reset();
    readouts();
  }
  $('strike').onclick = strikeOrReset;
  $('breeze').onclick = () => eng.blow(0.35, 1.5);
  $('blow').onclick = () => eng.blow(5, 0.4); // 4 m/s for 0.4 s no longer puts it out at the 1/240 step; 5 does, the breeze never
  let drag = null;
  cv.addEventListener('pointerdown', e => {
    if (eng.stats.phase !== 'lit') { strikeOrReset(); return; }
    cv.setPointerCapture(e.pointerId); drag = { x: e.offsetX, y: e.offsetY, t: performance.now() };
  });
  cv.addEventListener('pointermove', e => {
    if (!drag) return;
    const now = performance.now(), dt = Math.max(8, now - drag.t) / 1000, k = (cw / cv.clientWidth) / s; // css px -> cells
    const vx = (e.offsetX - drag.x) * k / dt, vy = (e.offsetY - drag.y) * k / dt;
    eng.poke(e.offsetX * k, e.offsetY * k, Math.max(-6000, Math.min(6000, vx * 0.7)), Math.max(-6000, Math.min(6000, vy * 0.7)));
    drag = { x: e.offsetX, y: e.offsetY, t: now };
  });
  const end = () => { drag = null; };
  cv.addEventListener('pointerup', end); cv.addEventListener('pointercancel', end);

  const viewButtons = document.querySelectorAll('.views button');
  viewButtons.forEach(b => b.onclick = () => {
    view = b.dataset.view; eng.setView(view);
    viewButtons.forEach(x => x.setAttribute('aria-pressed', x === b));
  });
  const bind = (id, fn) => { const el = $(id); const run = () => fn(+el.value); el.addEventListener('input', run); run(); };
  bind('ev', v => { cam.expo = EXPO0 * Math.pow(2, v); eng.setCamera(cam.expo, cam.wb, CHEMI); $('evOut').textContent = `${v >= 0 ? '+' : '−'}${Math.abs(v).toFixed(1)} EV`; });
  bind('wb', v => { cam.wb = v; eng.setCamera(cam.expo, cam.wb, CHEMI); $('wbOut').textContent = `${v} K`; });
  bind('o2', v => { room.o2 = v / 100; eng.setRoom(room.o2, room.g, room.draft); $('o2Out').textContent = `${v.toFixed(1)} %`; });
  bind('g', v => { room.g = v; eng.setRoom(room.o2, room.g, room.draft); $('gOut').textContent = `${v.toFixed(2)} g`; });
  if (calm) $('draft').value = 0;
  bind('draft', v => { room.draft = v; eng.setRoom(room.o2, room.g, room.draft); $('draftOut').textContent = `${v.toFixed(1)}×`; });

  // ---------- loop ----------
  let last = performance.now(), roTimer = 0;
  const ema = (a, v) => a ? a * 0.92 + v * 0.08 : v;
  function frame(now) {
    const dt = Math.min(0.05, (now - last) / 1000);
    perf.frame = ema(perf.frame, now - last); last = now;
    const t0 = performance.now(); eng.step(dt);
    const t1 = performance.now(); eng.render();
    const t2 = performance.now(); draw();
    perf.sim = ema(perf.sim, t1 - t0); perf.shade = ema(perf.shade, t2 - t1);
    roTimer += dt; if (roTimer > 0.25) { roTimer = 0; readouts(); }
    requestAnimationFrame(frame);
  }
  draw(); readouts();
  requestAnimationFrame(frame);
  setTimeout(() => { if (eng.stats.phase === 'ready') { eng.strike(false); readouts(); } }, 700);
})();

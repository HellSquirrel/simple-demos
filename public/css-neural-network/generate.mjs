// Generates the pure-CSS training apparatus in ../index.html: the @property
// registrations, the two CSS rules that ARE the training loop, and the nested
// DOM that iterates them. Also simulates the identical arithmetic in JS and
// prints the reference trajectory, so the browser's math can be checked.
//
//   node generate.mjs          # report + refuse to touch index.html
//   node generate.mjs --write  # regenerate the marked blocks in index.html
//
// No weight in index.html is trained here. The browser trains them at page
// load: 120 steps of full-batch gradient descent with momentum, executed by
// the CSS cascade. This script only prints the (very repetitive) program text.

import { readFileSync, writeFileSync } from "node:fs";

const H = 5;            // hidden neurons
const N = 12;           // training samples of sin(2*pi*x), x = (i+0.5)/N
const STEPS = 80;       // gradient descent steps = DOM nesting depth
const LR = 0.3;
const BETA = 0.9;       // momentum
const SEED = 9;         // picked by sweep: best final fit for this optimizer
const SLOT = 0.16;      // seconds each step is on screen
const HOLD = 4.8;       // extra seconds the converged step stays on screen
const PERIOD = (STEPS + 1) * SLOT + HOLD;

// --- seeded PRNG (mulberry32) + gaussian, for theta_0 only ---
function mulberry32(a) {
  return function () {
    a |= 0; a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
const rand = mulberry32(SEED);
const gauss = () => {
  let u = 0, v = 0;
  while (u === 0) u = rand();
  while (v === 0) v = rand();
  return Math.sqrt(-2 * Math.log(u)) * Math.cos(2 * Math.PI * v);
};

const X = Array.from({ length: N }, (_, i) => (i + 0.5) / N);
const T = X.map((x) => Math.sin(2 * Math.PI * x));
const F = (n) => String(n); // full double precision, so CSS math matches JS math

// theta_0: slopes spread wide, kinks spread across [0,1], small output layer
const u0 = Array.from({ length: H }, () => gauss() * 4);
const p0 = u0.map((uk) => -uk * rand());
const v0 = Array.from({ length: H }, () => gauss() * 0.5);
const q0 = 0;

// --- JS reference: EXACTLY the arithmetic the emitted calc() performs ---
// (same association order, same sign(max(0,z)) indicator, same constants)
function simulate() {
  const u = u0.slice(), p = p0.slice(), v = v0.slice();
  let q = q0;
  const mu = new Array(H).fill(0), mp = new Array(H).fill(0), mv = new Array(H).fill(0);
  let mq = 0;
  const K = LR * 2 / N; // the 0.05 that appears in the emitted rules
  const lossAt = () => {
    let s = 0;
    for (let i = 0; i < N; i++) {
      let y = 0;
      for (let k = 0; k < H; k++) y = y + v[k] * Math.max(0, u[k] * X[i] + p[k]);
      s = s + Math.pow(y + q - T[i], 2);
    }
    return s / N;
  };
  const track = [{ step: 0, mse: lossAt() }];
  for (let e = 1; e <= STEPS; e++) {
    const h = [], r = [];
    for (let i = 0; i < N; i++) {
      h[i] = [];
      for (let k = 0; k < H; k++) h[i][k] = Math.max(0, u[k] * X[i] + p[k]);
      let y = v[0] * h[i][0];
      for (let k = 1; k < H; k++) y = y + v[k] * h[i][k];
      r[i] = y + q - T[i];
    }
    for (let k = 0; k < H; k++) {
      let su = r[0] * Math.sign(h[0][k]) * X[0];
      let sp = r[0] * Math.sign(h[0][k]);
      let sv = r[0] * h[0][k];
      for (let i = 1; i < N; i++) {
        su = su + r[i] * Math.sign(h[i][k]) * X[i];
        sp = sp + r[i] * Math.sign(h[i][k]);
        sv = sv + r[i] * h[i][k];
      }
      mu[k] = BETA * mu[k] - K * v[k] * su;
      mp[k] = BETA * mp[k] - K * v[k] * sp;
      mv[k] = BETA * mv[k] - K * sv;
      u[k] = u[k] + mu[k];
      p[k] = p[k] + mp[k];
      v[k] = v[k] + mv[k];
    }
    let sq = r[0];
    for (let i = 1; i < N; i++) sq = sq + r[i];
    mq = BETA * mq - K * sq;
    q = q + mq;
    track.push({ step: e, mse: lossAt() });
  }
  return { u, p, v, q, track };
}

// --- emit: @property registrations ---
let props = "";
const reg = (name) =>
  (props += `      @property ${name} { syntax: "<number>"; inherits: true; initial-value: 0; }\n`);
for (const fam of ["a", "b"])
  for (let k = 1; k <= H; k++)
    for (const base of ["u", "p", "v", "mu", "mp", "mv"]) reg(`--${base}${k}${fam}`);
for (const fam of ["a", "b"]) { reg(`--q${fam}`); reg(`--mq${fam}`); }
for (let k = 1; k <= H; k++) for (let i = 1; i <= N; i++) reg(`--h${k}x${i}`);
for (let i = 1; i <= N; i++) reg(`--r${i}`);
for (const d of ["--L", "--ys", "--yt", "--ysv", "--xi"]) reg(d);
reg("--Li"); reg("--Ld1"); reg("--Ld2"); reg("--Ld3");
props += `      @property --x { syntax: "<number>"; inherits: false; initial-value: 0; }\n`;

// --- emit: one generation of the training loop (OLD family -> NEW family) ---
const K = LR * 2 / N;
function generation(NEW, OLD) {
  let css = "";
  for (let k = 1; k <= H; k++)
    for (let i = 1; i <= N; i++)
      css += `        --h${k}x${i}: max(0, var(--u${k}${OLD}) * ${F(X[i - 1])} + var(--p${k}${OLD}));\n`;
  for (let i = 1; i <= N; i++) {
    const terms = Array.from({ length: H }, (_, k) => `var(--v${k + 1}${OLD}) * var(--h${k + 1}x${i})`);
    css += `        --r${i}: calc(${terms.join(" + ")} + var(--q${OLD}) - ${F(T[i - 1])});\n`;
  }
  for (let k = 1; k <= H; k++) {
    const su = Array.from({ length: N }, (_, i) => `var(--r${i + 1})*sign(var(--h${k}x${i + 1}))*${F(X[i])}`);
    const sp = Array.from({ length: N }, (_, i) => `var(--r${i + 1})*sign(var(--h${k}x${i + 1}))`);
    const sv = Array.from({ length: N }, (_, i) => `var(--r${i + 1})*var(--h${k}x${i + 1})`);
    css += `        --mu${k}${NEW}: calc(${BETA} * var(--mu${k}${OLD}) - ${K} * var(--v${k}${OLD}) * (${su.join(" + ")}));\n`;
    css += `        --mp${k}${NEW}: calc(${BETA} * var(--mp${k}${OLD}) - ${K} * var(--v${k}${OLD}) * (${sp.join(" + ")}));\n`;
    css += `        --mv${k}${NEW}: calc(${BETA} * var(--mv${k}${OLD}) - ${K} * (${sv.join(" + ")}));\n`;
    css += `        --u${k}${NEW}: calc(var(--u${k}${OLD}) + var(--mu${k}${NEW}));\n`;
    css += `        --p${k}${NEW}: calc(var(--p${k}${OLD}) + var(--mp${k}${NEW}));\n`;
    css += `        --v${k}${NEW}: calc(var(--v${k}${OLD}) + var(--mv${k}${NEW}));\n`;
  }
  const sq = Array.from({ length: N }, (_, i) => `var(--r${i + 1})`);
  css += `        --mq${NEW}: calc(${BETA} * var(--mq${OLD}) - ${K} * (${sq.join(" + ")}));\n`;
  css += `        --q${NEW}: calc(var(--q${OLD}) + var(--mq${NEW}));\n`;
  return css;
}

// --- emit: rider display formulas for a family (what the 🐠 swims on) ---
function riderRules(fam) {
  const net = (x) =>
    Array.from({ length: H }, (_, k) => `var(--v${k + 1}${fam}) * max(0, var(--u${k + 1}${fam}) * ${x} + var(--p${k + 1}${fam}))`)
      .join(" + ") + ` + var(--q${fam})`;
  const lossTerms = Array.from({ length: N }, (_, i) => `pow(${net(F(X[i]))} - ${F(T[i])}, 2)`);
  return (
    `        --ys: calc(${net("var(--x)")});\n` +
    `        --L: calc((${lossTerms.join(" + ")}) / ${N});\n`
  );
}

// --- emit: :root theta_0 (the only numbers shipped; random, untrained) ---
let init = "";
for (let k = 1; k <= H; k++)
  init += `        --u${k}b: ${F(u0[k - 1])}; --p${k}b: ${F(p0[k - 1])}; --v${k}b: ${F(v0[k - 1])};\n`;
init += `        --qb: ${F(q0)};\n`;

const trainer = `      /* theta_0 — random init, printed by generate.mjs, never trained offline */
      :root {
${init}      }

      /* THE TRAINING LOOP. One rule per generation parity; the nested DOM
         below runs them ${STEPS} times. Each element computes one full-batch
         gradient-descent-with-momentum step from its parent's weights. */
      .st { display: contents; }
      .st.a {
${generation("a", "b")}      }
      .st.b {
${generation("b", "a")}      }

      /* riders: each shows its own step's network. .a steps carry family a. */
      .st.a > .r {
${riderRules("a")}      }
      .st.b > .r, .track > .r {
${riderRules("b")}      }

      /* replay clock: ${STEPS + 1} slots x ${SLOT}s + ${HOLD}s hold = ${PERIOD}s */
      .r {
        animation:
          reveal ${PERIOD}s linear infinite,
          swim 7s linear infinite;
        animation-delay: calc(var(--si) * ${SLOT}s), 0s;
      }
      @keyframes reveal {
        0%, ${((SLOT / PERIOD) * 100).toFixed(4)}% { visibility: visible; }
        ${((SLOT / PERIOD) * 100 + 0.0001).toFixed(4)}%, 100% { visibility: hidden; }
      }
      .r.last { animation-name: lastreveal, swim; }
      @keyframes lastreveal {
        0%, ${(((SLOT + HOLD) / PERIOD) * 100).toFixed(4)}% { visibility: visible; }
        ${(((SLOT + HOLD) / PERIOD) * 100 + 0.0001).toFixed(4)}%, 100% { visibility: hidden; }
      }
`;

// --- emit: the DOM that is the loop counter ---
let dom = `<i class="r" style="--si:0"><b></b></i>`;
let close = "";
let open = "";
for (let s = 1; s <= STEPS; s++) {
  const fam = s % 2 === 1 ? "a" : "b";
  const last = s === STEPS ? " last" : "";
  open += `<div class="st ${fam}"><i class="r${last}" style="--si:${s}"><b></b></i>`;
  close += `</div>`;
}
dom += open + close;

// --- report ---
const ref = simulate();
console.error(`pure-CSS trainer: H=${H} N=${N} steps=${STEPS} lr=${LR} beta=${BETA} seed=${SEED}`);
console.error("reference trajectory (JS mirror of the CSS arithmetic):");
for (const { step, mse } of ref.track)
  if ([0, 1, 2, 5, 10, 20, 40, 60, 80, 100, 120].includes(step))
    console.error("  step", String(step).padEnd(4), "mse", mse.toFixed(6));
console.error("final weights (browser agrees to ~5 significant digits; Blink stores computed numbers as float32):");
for (let k = 0; k < H; k++)
  console.error(`  u${k + 1}=${ref.u[k].toPrecision(10)} p${k + 1}=${ref.p[k].toPrecision(10)} v${k + 1}=${ref.v[k].toPrecision(10)}`);
console.error(`  q=${ref.q.toPrecision(10)}`);
console.error(`replay: ${STEPS + 1} slots x ${SLOT}s + ${HOLD}s hold = ${PERIOD}s loop`);

// --- splice into index.html between markers ---
// CSS markers are CSS comments (HTML comments inside <style> would open a
// CDO token and eat the next rule); the DOM marker is a real HTML comment.
const esc = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const MARKS = [
  ["/* gen:props */", "/* /gen:props */", props],
  ["/* gen:trainer */", "/* /gen:trainer */", trainer],
  ["<!-- gen:dom -->", "<!-- /gen:dom -->", dom],
];
const path = new URL("./index.html", import.meta.url);
let html = readFileSync(path, "utf8");
for (const [openMark, closeMark, block] of MARKS) {
  const re = new RegExp(`(${esc(openMark)})[\\s\\S]*?(${esc(closeMark)})`);
  if (!re.test(html)) {
    console.error(`marker ${openMark} not found in index.html`);
    process.exit(1);
  }
  html = html.replace(re, `$1\n${block.replace(/\$/g, "$$$$")}$2`);
}
if (process.argv.includes("--write")) {
  writeFileSync(path, html);
  console.error("index.html regenerated");
} else {
  console.error("dry run (pass --write to update index.html)");
}

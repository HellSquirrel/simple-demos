# flame — Blackbody Candle engine (Rust → WebAssembly)

The reacting-flow candle flame and its Planck/CIE light model, compiled to `wasm32-unknown-unknown`.
Plain C ABI, no `wasm-bindgen`, no imports. JS calls the exported `fl_*` functions and reads the
linear-light image, the bloom and the readings straight from `memory`.

## Build

```sh
rustup target add wasm32-unknown-unknown
cargo build --release --target wasm32-unknown-unknown
# -> target/wasm32-unknown-unknown/release/flame.wasm  (~76 KB)
cargo run --release --example prof     # native per-stage profile
```

`.cargo/config.toml` enables `simd128`, `bulk-memory` and `nontrapping-fptoint`.

## Using it from JS

```js
const { instance } = await WebAssembly.instantiate(bytes, {});
const w = instance.exports;
w.fl_init();                               // builds the tables, resets the candle
w.fl_set_camera(0.36, 2800, 0.00022);      // exposure, white balance (K), blue-zone gain
w.fl_set_room(0.209, 1.0, 1.0, 1.0);       // O2 fraction, gravity (g), swirl, room draft
w.fl_strike();

function frame(dt) {
  w.fl_step(dt);                           // substeps of 1/240 s, at most 25 ms of flame time per frame
  w.fl_render();                           // linear-light RGB at 168 x 280, the bloom at 42 x 70, and the stats
  const lin = new Float32Array(w.memory.buffer, w.fl_linear(), 168 * 280 * 3);  // upload as RGB32F and tone-map in a shader
}
```

| Export | What it does |
| --- | --- |
| `fl_step(dt)` | Advance the simulation |
| `fl_render()` | Shade every cell (soot Planck × CIE + CH*/C₂* bands), project along the line of sight, compute the bloom |
| `fl_strike()`, `fl_reset()` | Light the candle / a fresh candle |
| `fl_blow(m_per_s, seconds)`, `fl_poke(x, y, vx, vy)` | Gust; push the air (cell units) |
| `fl_set_view(v)` | 0 camera, 1 temperature, 2 soot, 3 reaction |
| `fl_linear()`, `fl_bloom()` | `f32` RGB buffers; sizes from `fl_dims()` and `fl_bloomdims()` (width << 16 \| height) |
| `fl_stats()` | `f32[16]`: X Y Z, soot T, light centroid x y, wick tip y, phase, heat feedback, wick T, seconds since lit, heat release, radiated heat, time |
| `fl_matrix()` | `f32[9]`: XYZ → linear sRGB with the current white balance |
| `fl_grey_rgb(T, gain)` + `fl_scratch()` | Colour of an opaque grey body (the wick tip) |
| `fl_visible_share(T)` | Fraction of soot radiation between 380 and 780 nm |

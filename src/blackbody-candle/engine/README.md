# flame — Blackbody Candle engine (Rust → WebAssembly)

The reacting-flow match flame and its Planck/CIE light model, compiled to `wasm32-unknown-unknown`.
Plain C ABI, no `wasm-bindgen`, no imports. JS calls the exported `fl_*` functions and reads
pixels, readings and sparks straight from `memory`.

## Build

```sh
rustup target add wasm32-unknown-unknown
cargo build --release --target wasm32-unknown-unknown
# -> target/wasm32-unknown-unknown/release/flame.wasm  (~70 KB)
```

`.cargo/config.toml` enables `simd128`, `bulk-memory` and `nontrapping-fptoint`
(all baseline in current browsers).

Native profile of the stages:

```sh
cargo run --release --example prof
```

## Using it from JS

```js
const { instance } = await WebAssembly.instantiate(bytes, {});
const w = instance.exports;
w.fl_init();                               // builds spectral tables, resets the match
w.fl_set_camera(4.2, 2800, 0.00022);       // exposure, white balance (K), blue-zone gain
w.fl_set_room(0.209, 1.0, 1.0, 1.0);       // O2 fraction, gravity (g), swirl, room draft
w.fl_strike(1);                            // 1 = with sparks

function frame(dt) {
  w.fl_step(dt);                           // substeps of 1/240 s, at most 25 ms of flame time per frame
  w.fl_render();                           // RGBA at 168 x 280 + stats
  const px = new Uint8ClampedArray(w.memory.buffer, w.fl_pixels(), 168 * 280 * 4);
  ctx.putImageData(new ImageData(px, 168, 280), 0, 0);
}
```

| Export | What it does |
| --- | --- |
| `fl_step(dt)` | Advance the simulation |
| `fl_render()` | Shade every cell (soot Planck x CIE + CH*/C2* bands), 2x upsample, tone map |
| `fl_strike(sparks)`, `fl_reset()` | Light the match / fresh match |
| `fl_blow(m_per_s, seconds)`, `fl_poke(x, y, vx, vy)` | Gust; push the air (cell units) |
| `fl_set_view(v)` | 0 camera, 1 temperature, 2 soot, 3 reaction |
| `fl_stats()` | `f32[24]`: X Y Z, soot T, light centroid x y, head y, phase, h, ember T, burned, since, q_rel, q_rad, n sparks, time |
| `fl_sparks()` | `f32[64*5]`: x, y, prev x, prev y, T |
| `fl_matrix()` | `f32[9]`: XYZ -> linear sRGB with the current white balance |
| `fl_grey_rgb(T, gain)` + `fl_scratch()` | Colour of an opaque grey body (ember, sparks) |
| `fl_visible_share(T)` | Fraction of soot radiation between 380 and 780 nm |

## What changed from the JS version

Same model and constants; the readings agree to within a few kelvin. The speed comes from:

- one fused semi-Lagrangian pass: T, fuel, O2, soot, u and v all ride the same velocity, so each
  cell is traced back once and all six fields are sampled there;
- red-black Gauss-Seidel for diffusion (the in-order sweep was latency-bound: every cell waited on
  the one just written);
- tone curve + sRGB as a 64K lookup table, separable 2x upsample with a precomputed tap table;
- `f32` everywhere, no allocation after `fl_init`, so JS views into `memory` stay valid.

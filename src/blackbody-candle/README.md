# Blackbody Candle

A candle you can light, where nothing about the flame is hand-drawn. Its shape comes from a small
reacting, buoyant gas simulation, and every colour comes from Planck's law seen through the CIE 1931
colour-matching curves. The engine is written in Rust and runs in the browser as WebAssembly.

It is a physics-based reworking of Carmen Ansio's
[A flame needs three sine waves](https://www.carmenansio.com/articles/matchstick-flame-canvas/),
which builds the same match from three teardrop shapes and three sine waves.

## Short description (for a post or a repo)

> A candle flame simulated in the browser: buoyant, reacting gas on a 0.25 mm grid, soot that glows
> by Planck's law, and colour computed through the CIE 1931 observer with camera white balance.
> Light it, wave the air, blow it out, change the oxygen or gravity. Physics and pixel shading are
> Rust compiled to a 70 KB WebAssembly module.

## Open it

The built page is `public/blackbody-candle/index.html` at the root of the simple-demos repo: a single
self-contained file (≈150 KB, the wasm is embedded). Open it in any modern browser, no server needed.

## Project layout

```
src/blackbody-candle/        this folder (sources; nothing here is served)
  web/index.template.html   markup + CSS, two script placeholders
  web/app.js                UI, canvas drawing, spectrum chart
  web/engine.js             the JS side of the wasm engine's C ABI
  engine/                   Rust crate -> flame.wasm  (see engine/README.md for the ABI)
    src/lib.rs              simulation, spectral tables, white balance, shading
    examples/prof.rs        native per-stage profiler
  tools/build.py            cargo build + assemble public/blackbody-candle/index.html
  tools/bench.js            Node: per-frame timings and readings
  tools/snapshot.js         Node: headless frames + readings, for tuning without a browser
  tools/browser_check.py    Playwright: screenshots, timings, blow-out check
  HANDOFF.md                full context for continuing the work
public/blackbody-candle/     the built page + og.png (served as-is)
```

## Build

Run from `src/blackbody-candle/`:

```sh
rustup target add wasm32-unknown-unknown
python3 tools/build.py          # cargo build --release + writes public/blackbody-candle/index.html
node tools/bench.js             # timings and readings
node tools/snapshot.js          # headless frames -> snapshot.ppm
python3 tools/browser_check.py  # needs: pip install playwright && playwright install chromium
```

`python3 tools/build.py --no-cargo` reassembles the page from the last wasm build.

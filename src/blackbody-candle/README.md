# Blackbody Candle

A candle you can light, where nothing about the flame is hand-drawn. Its shape comes from a small
reacting, buoyant gas simulation, and every colour comes from Planck's law seen through the CIE 1931
colour-matching curves. The engine is written in Rust and runs in the browser as WebAssembly; the GPU
resamples and tone-maps its output. The idea of building a flame instead of drawing one comes from
Carmen Ansio's [A flame needs three sine waves](https://www.carmenansio.com/articles/matchstick-flame-canvas/).

## Layout

```
web/index.template.html   markup + CSS, two script placeholders
web/app.js                UI, the WebGL2 presenter, the candle and chart drawing
web/engine.js             the JS side of the wasm engine's C ABI
engine/                   Rust crate -> flame.wasm  (see engine/README.md for the ABI)
  src/lib.rs              simulation, spectral tables, white balance, line-of-sight projection
  examples/prof.rs        native per-stage profiler
tools/build.py            cargo build + assemble ../../public/blackbody-candle/index.html
tools/bench.js            Node: per-frame timings and readings
tools/snapshot.js         Node: headless frames and readings, and a measure of the flame's dance
```

The built page, `public/blackbody-candle/index.html` at the repo root, is a single file with the
wasm embedded; it is committed, so deploying never needs Rust.

## Build

Run from this folder:

```sh
rustup target add wasm32-unknown-unknown
python3 tools/build.py          # cargo build --release + writes the page
python3 tools/build.py --no-cargo   # reassemble the page from the last wasm build
node tools/bench.js
node tools/snapshot.js
cd engine && cargo run --release --example prof
```

## The model, briefly

A 42 x 70 mm window of air on a grid of 0.25 mm cells, stepped every 1/240 s. Each substep: buoyancy,
a wandering room wind with small eddies, vorticity confinement; a wick model (a curled hot surface that
feeds wax vapour at a rate set by the heat it gets back from the flame, with a noisy capillary feed);
a pressure projection with the thermal expansion of burning gas; semi-Lagrangian advection of
temperature, fuel, oxygen, soot and velocity with midpoint-rule departure points; implicit diffusion
with coefficients that scale as T^1.7 (Prandtl 0.71, Lewis 3 for the fuel vapour); an out-of-plane
entrainment term standing in for the third dimension; one-step Arrhenius chemistry with per-kelvin
rate tables, soot inception and oxidation, and optically thin radiation integrated exactly.

Light: per cell, soot XYZ from a table of ∫ B(λ,T)/λ · CMF(λ) dλ (soot emissivity ∝ 1/λ) plus CH*,
C₂* and CO₂* band light ∝ reaction rate; the slice is projected along the line of sight about each
row's centre of light (a swaying flame is symmetric about its own bent axis); Bradford white balance
from a blackbody white to D65; XYZ → linear sRGB; then, on the GPU, Catmull-Rom resampling per device
pixel, a small bloom, ACES and sRGB. The wick tip is an opaque grey body.

Known limits: a 2D slice with an entrainment stand-in; one-step chemistry; the vapour core is warmer
and sootier than a real candle's dark zone; constants tuned for an 18 mm flame with a 2,200 K adiabatic
flame temperature. Trends are physical, absolute numbers are not predictions.

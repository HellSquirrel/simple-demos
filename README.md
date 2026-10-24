# Simple Demos

A collection of simple demos built without any libraries or frameworks — just pure CSS, HTML, and JS.

## Demos

- [`css-monte-carlo`](public/css-monte-carlo/) — *Precisely random(): Integrals in Pure CSS.* Monte Carlo integration told with confetti, in pure CSS with zero JavaScript, built on the newly shipped CSS `random()`. A 🎉 button throws confetti; 500 pieces thrown at a circle measure π; a thousand pieces estimate the integrals of four functions (two with no elementary antiderivative), with the error after 10, 100 and 1 000 throws; and a single piece is thrown ten times a second, forever. Each piece is an empty `<i>` whose registered custom properties keep its own `random()` coordinates, a `calc()` hit test (`sign()` with `sqrt()`/`sin()`/`exp()`/`pow()`) colours it, and weighted `counter-increment`s add the hits straight into a fixed-point estimate. The buttons get fresh draws by swapping `::before` for `::after` through two hidden radios. The single piece walks three coprime-length lists of named-key `random()` values with step animations, and keeps score in animation time: a stopwatch animation that `@container style(--hit: 1)` pauses and resumes. Needs Chrome 156+ or Safari 26.2+.
- [`css-neural-network`](public/css-neural-network/) — a neural network trained **live, in pure CSS** — no JavaScript on the page and none in a build step. The three parameters live in scrollbars (scroll position is the state), `scroll-timeline` reads them, `calc()` computes real residuals/gradients from the training data at runtime, and `scroll-snap-type: mandatory` writes the update — one heavy-ball momentum step per frame, forever. A goldfish 🐠 learns to swim like `sin(x)`. Needs a recent Chrome; elsewhere the tank freezes over for the season.
- [`blackbody-candle`](public/blackbody-candle/) — *How Many Rust Engines Does It Take to Light a Candle?* A candle you can light, where nothing about the flame is hand-drawn. Its shape comes from a small reacting, buoyant gas simulation (168 × 280 cells of 0.25 mm; buoyancy, pressure projection, one-step chemistry, soot) and every colour comes from Planck’s law seen through the CIE 1931 colour-matching curves, with camera white balance. Light it, wave the air, blow it out, change the oxygen or gravity. The engine is Rust compiled to a 70 KB WebAssembly module; JavaScript draws the canvas. Sources in [`src/blackbody-candle/`](src/blackbody-candle/). A physics-based reworking of Carmen Ansio’s [A flame needs three sine waves](https://www.carmenansio.com/articles/matchstick-flame-canvas/).
- [`random-post-button`](public/random-post-button/) — a button that links to a random post, in HTML & CSS only (zero JavaScript). Three entropy sources standing in for the not-yet-shipped CSS `random()`: an infinite clock cycle frozen on hover/focus, a scroll-driven pick, and a mouse-path latch — plus a rolling 3D die as a status indicator. A riff on ["A Button That Links to a Random Post in HTML & CSS Only"](https://master.dev/blog/a-button-that-links-to-a-random-post-in-html-css-only/) by Chris Coyier.

## Structure

```
public/          # site root, served as-is
├── index.html   # landing page listing all demos
└── <demo>/      # one folder per demo
src/<demo>/      # sources for demos that need a build (currently only blackbody-candle)
og/              # sources for the social-preview cards
```

Most demos are hand-written files in `public/`. `blackbody-candle` is built: `python3 src/blackbody-candle/tools/build.py` compiles the Rust engine to wasm (needs `rustup target add wasm32-unknown-unknown`) and assembles `public/blackbody-candle/index.html`; the built page is committed, so deploying never needs Rust.

## Deployment

Deployed to Cloudflare via [Workers Static Assets](https://developers.cloudflare.com/workers/static-assets/) — no build step, no Worker code.

```sh
pnpm dlx wrangler deploy
```

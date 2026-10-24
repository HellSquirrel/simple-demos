# Simple Demos

A collection of simple demos built without any libraries or frameworks — just pure CSS, HTML, and JS.

## Demos

Live at [acorns.nutlore.me](https://acorns.nutlore.me/).

- [Precisely random(): Integrals in Pure CSS](https://acorns.nutlore.me/css-monte-carlo/) — Can we do calculus in pure CSS? Easy, especially now that CSS has `random()`. Throw confetti, count what lands under the curve, and out come π and integrals with no elementary antiderivative. ([source](public/css-monte-carlo/))
- [A JS-less Neural Network on… Scrollbars](https://acorns.nutlore.me/css-neural-network/) — a neural network trained live by pure CSS: scroll positions as parameters, mandatory snap as the optimizer, the render loop as the clock. A goldfish learns to swim. Zero JavaScript. ([source](public/css-neural-network/))
- [How Many Rust Engines Does It Take to Light a Candle?](https://acorns.nutlore.me/blackbody-candle/) — one, it turns out, plus a little bit of physics: a simulation of the hot gas above the wick, and some rendering that turns it into light you can actually see, all packed into a WebAssembly engine written in Rust, plus a couple of WebGL shaders that put the pixels on screen. Inspired by Carmen Ansio’s [A flame needs three sine waves](https://www.carmenansio.com/articles/matchstick-flame-canvas/). ([source](src/blackbody-candle/))
- [A Random-Post Button, Now With One (1) Link](https://acorns.nutlore.me/random-post-button/) — a CSS-only random-link button: three ways to fake randomness without `random()`, plus a rolling 3D die. Inspired by Chris Coyier’s ["A Button That Links to a Random Post in HTML & CSS Only"](https://master.dev/blog/a-button-that-links-to-a-random-post-in-html-css-only/). ([source](public/random-post-button/))

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

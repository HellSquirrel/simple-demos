# Simple Demos

A collection of simple demos built without any libraries or frameworks — just pure CSS, HTML, and JS.

## Demos

- [`random-post-button`](public/random-post-button/) — a button that links to a random post, in HTML & CSS only (zero JavaScript). Three entropy sources standing in for the not-yet-shipped CSS `random()`: an infinite clock cycle frozen on hover/focus, a scroll-driven pick, and a mouse-path latch — plus a rolling 3D die as a status indicator. A riff on ["A Button That Links to a Random Post in HTML & CSS Only"](https://master.dev/blog/a-button-that-links-to-a-random-post-in-html-css-only/) by Chris Coyier.

## Structure

```
public/          # site root, served as-is
├── index.html   # landing page listing all demos
└── <demo>/      # one folder per demo
```

## Deployment

Deployed to Cloudflare via [Workers Static Assets](https://developers.cloudflare.com/workers/static-assets/) — no build step, no Worker code.

```sh
pnpm dlx wrangler deploy
```

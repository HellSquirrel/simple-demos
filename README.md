# Simple Demos

A collection of simple demos built without any libraries or frameworks — just pure CSS, HTML, and JS.

## Demos

_Coming soon._

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

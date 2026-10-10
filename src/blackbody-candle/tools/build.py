#!/usr/bin/env python3
"""Build the single-file page public/blackbody-candle/index.html (at the repo root).

1. Compiles the Rust engine to wasm (skip with --no-cargo to reuse the last build).
2. Inlines web/engine.js, the wasm as base64, and web/app.js into web/index.template.html.

Usage:  python3 tools/build.py [--no-cargo]
"""
import base64, pathlib, subprocess, sys

ROOT = pathlib.Path(__file__).resolve().parent.parent  # src/blackbody-candle
REPO = ROOT.parent.parent  # the simple-demos checkout
ENGINE = ROOT / "engine"
WASM = ENGINE / "target/wasm32-unknown-unknown/release/flame.wasm"
SRC = ROOT / "web"
OUT = REPO / "public/blackbody-candle/index.html"


def cargo():
    cmd = ["cargo", "build", "--release", "--target", "wasm32-unknown-unknown"]
    print("$", " ".join(cmd))
    subprocess.run(cmd, cwd=ENGINE, check=True)


def main():
    if "--no-cargo" not in sys.argv:
        cargo()
    if not WASM.exists():
        sys.exit(f"missing {WASM}; build the engine first")
    b64 = base64.b64encode(WASM.read_bytes()).decode()
    engine_js = (SRC / "engine.js").read_text() + f'\nconst WASM_B64 = "{b64}";\n'
    page = (SRC / "index.template.html").read_text()
    page = page.replace("/*__ENGINE_JS__*/", engine_js).replace("/*__APP__*/", (SRC / "app.js").read_text())
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(page)
    print(f"wrote {OUT.relative_to(REPO)}  ({len(page) / 1024:.0f} KB, wasm {WASM.stat().st_size / 1024:.0f} KB)")


if __name__ == "__main__":
    main()

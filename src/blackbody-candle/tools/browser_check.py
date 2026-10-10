#!/usr/bin/env python3
"""Headless browser check of public/blackbody-candle/index.html with Playwright (Chromium).

- screenshots at desktop and phone width, plus the Temperature view
- engine timings, then a "Blow out" to check extinction
- reports any page errors

Usage:  pip install playwright && playwright install chromium
        python3 tools/browser_check.py [out_dir]
Note: headless Chromium draws the canvas in software, so "Whole frame" here is much slower
than in a normal browser with a GPU. Read the engine timings, not the frame rate.
"""
import asyncio, pathlib, sys
from playwright.async_api import async_playwright

ROOT = pathlib.Path(__file__).resolve().parent.parent
PAGE = (ROOT.parent.parent / "public/blackbody-candle/index.html").as_uri()
OUT = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "docs/screenshots"
READ = "['pfSim','pfShade','pfFrame','pfFps','roT','statusText'].map(i=>document.getElementById(i).innerText.replace(/\\n/g,' '))"


async def main():
    OUT.mkdir(parents=True, exist_ok=True)
    async with async_playwright() as p:
        b = await p.chromium.launch()
        errors = []
        # desktop
        pg = await b.new_page(viewport={"width": 1200, "height": 1000})
        pg.on("pageerror", lambda e: errors.append(str(e)))
        await pg.goto(PAGE)
        await pg.wait_for_timeout(4500)
        await pg.screenshot(path=str(OUT / "desktop.png"))
        await pg.locator(".frame").screenshot(path=str(OUT / "flame-camera.png"))
        await pg.click("button[data-view=temp]"); await pg.wait_for_timeout(400)
        await pg.locator(".frame").screenshot(path=str(OUT / "flame-temperature.png"))
        await pg.click("button[data-view=camera]")
        print("lit  ", await pg.evaluate(READ))
        await pg.click("#breeze"); await pg.wait_for_timeout(800)
        await pg.locator(".frame").screenshot(path=str(OUT / "flame-breeze.png"))
        await pg.wait_for_timeout(2500)
        await pg.click("#blow"); await pg.wait_for_timeout(2500)
        print("blown", await pg.evaluate(READ))
        # phone
        ph = await b.new_page(viewport={"width": 390, "height": 1500}, device_scale_factor=2)
        ph.on("pageerror", lambda e: errors.append(str(e)))
        await ph.goto(PAGE); await ph.wait_for_timeout(4000)
        await ph.screenshot(path=str(OUT / "phone.png"))
        print("errors", errors or "none")
        await b.close()


asyncio.run(main())

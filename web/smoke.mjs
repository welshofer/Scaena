// PLAN 2.1 smoke check: the web player in headless Chromium, the engine in its worker.
//
//   node web/smoke.mjs [out-dir]     (after `just web`; from the repository's root)
//
// The built player opens the torture deck twice: with `?painter=gpu`, which must paint with
// vello on WebGPU into the worker's OffscreenCanvas, and with `?painter=cpu`, whose vello_cpu
// frames go onto the canvas by its 2D context. Each time it shows every frame the golden rasters
// hold (tests/golden/torture/raw.fnv1a names them: a state at rest, a fraction of the cue into
// it, another format), and the frame on the page is screenshotted. Each must be the canvas's
// size, opaque, and more than one color. The frames are saved as
// <out-dir>/player-webgpu/<frame>.png and <out-dir>/player-cpu/<frame>.png (default out-dir
// target/web-smoke), where the parity harness holds them to the goldens (SPEC §13.5; `just
// web-smoke` runs it). Exits 1 on any failure. Needs Playwright and its Chromium (serve.mjs).
import { join } from "node:path";
import { goldenFrames, launch, serve, shoot } from "./serve.mjs";

const out = process.argv[2] ?? "target/web-smoke";
const server = await serve();
const player = (painter) => `${server.origin}/web/dist/?painter=${painter}`;

const frames = await goldenFrames();
const browser = await launch();
const failures = [];
try {
  for (const [painter, expected] of [["gpu", "webgpu"], ["cpu", "cpu"]]) {
    // Tall enough for the torture deck's 9:16 format at its own size, under the status line.
    const page = await browser.newPage({ viewport: { width: 1920, height: 2000 }, deviceScaleFactor: 1 });
    page.on("console", (m) => m.type() === "error" && failures.push(`${painter}: console: ${m.text()}`));
    page.on("pageerror", (e) => failures.push(`${painter}: ${e.message}`));
    const navigated = performance.now();
    await page.goto(player(painter));
    await page.waitForFunction(() => window.scaena || document.querySelector("#status").textContent.startsWith("error"), null, { timeout: 120000 });
    const opened = await page.evaluate(() => window.scaena && { painter: window.scaena.painter, adapter: window.scaena.adapter });
    if (!opened) throw new Error(`${painter}: ${await page.textContent("#status")}`);
    console.log(`${painter}: ${opened.painter} (${opened.adapter}); first frame ${(performance.now() - navigated).toFixed(0)} ms after navigation`);
    if (opened.painter !== expected) failures.push(`${painter}: painted by ${opened.painter}, not ${expected}`);
    const ms = await shoot(page, frames, join(out, `player-${expected}`), painter, failures);
    ms.sort((a, b) => a - b);
    console.log(`${painter}: frames painted in ${ms[0].toFixed(1)}–${ms.at(-1).toFixed(1)} ms, median ${ms[ms.length >> 1].toFixed(1)}`);
    await page.close();
  }
} catch (e) {
  failures.push(String(e));
} finally {
  await browser.close();
  server.close();
}
console.log(
  failures.length
    ? `${failures.length} failure(s): ${failures.join("; ")}`
    : `all ${frames.length} frames shown in the player, painted in its worker by WebGPU and by the CPU painter`,
);
process.exit(failures.length ? 1 : 0);

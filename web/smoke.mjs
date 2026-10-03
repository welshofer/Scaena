// PLAN 2.1 smoke check: the web player in headless Chromium, the engine in its worker.
//
//   node web/smoke.mjs [out-dir]     (after `just web`; from the repository's root)
//
// The built player opens the torture deck twice: with `?painter=gpu`, which must paint with
// vello on WebGPU into the worker's OffscreenCanvas, and with `?painter=cpu`, whose vello_cpu
// frames reach the canvas as ImageBitmaps. Each time it shows every frame the golden rasters
// hold (tests/golden/torture/raw.fnv1a names them: a state at rest, a fraction of the cue into
// it, another format), and the frame on the page is screenshotted. Each must be the canvas's
// size, opaque, and more than one color. The frames are saved as
// <out-dir>/player-webgpu/<frame>.png and <out-dir>/player-cpu/<frame>.png (default out-dir
// target/web-smoke), where the parity harness holds them to the goldens (SPEC §13.5; `just
// web-smoke` runs it). Exits 1 on any failure. Needs Playwright and its Chromium (serve.mjs).
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { inflateSync } from "node:zlib";
import { launch, serve } from "./serve.mjs";

const out = process.argv[2] ?? "target/web-smoke";
const server = await serve();
const player = (painter) => `${server.origin}/web/dist/?painter=${painter}`;

/** A PNG's pixels as RGBA: 8-bit RGB or RGBA, not interlaced, as Chromium writes them. */
function decode(png) {
  let at = 8;
  let width = 0, height = 0, channels = 0;
  const idat = [];
  while (at < png.length) {
    const length = png.readUInt32BE(at);
    const kind = png.toString("latin1", at + 4, at + 8);
    const data = png.subarray(at + 8, at + 8 + length);
    if (kind === "IHDR") {
      [width, height] = [data.readUInt32BE(0), data.readUInt32BE(4)];
      channels = { 2: 3, 6: 4 }[data[9]] ?? 0;
      if (data[8] !== 8 || !channels || data[12] !== 0) throw new Error("expected an 8-bit RGB(A) PNG, not interlaced");
    } else if (kind === "IDAT") idat.push(data);
    at += 12 + length;
  }
  const raw = inflateSync(Buffer.concat(idat));
  const stride = width * channels;
  const rows = Buffer.alloc(stride * height);
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    for (let x = 0; x < stride; x++) {
      const a = x >= channels ? rows[y * stride + x - channels] : 0;
      const b = y > 0 ? rows[(y - 1) * stride + x] : 0;
      const c = x >= channels && y > 0 ? rows[(y - 1) * stride + x - channels] : 0;
      const p = a + b - c;
      const [pa, pb, pc] = [Math.abs(p - a), Math.abs(p - b), Math.abs(p - c)];
      const paeth = pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
      rows[y * stride + x] = raw[y * (stride + 1) + 1 + x] + [0, a, b, (a + b) >> 1, paeth][filter];
    }
  }
  const rgba = Buffer.alloc(width * height * 4, 255);
  for (let i = 0; i < width * height; i++) rows.copy(rgba, i * 4, i * channels, (i + 1) * channels);
  return { width, height, rgba };
}

const frames = (await readFile("tests/golden/torture/raw.fnv1a", "utf8")).trim().split("\n").map((l) => l.split(" ")[0]);
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
    const dir = join(out, `player-${expected}`);
    await mkdir(dir, { recursive: true });
    const ms = [];
    for (const name of frames) {
      // `state` at rest, or `state@fraction`: that fraction of the cue into it; after `~`, in
      // one of the deck's formats (`9x16` for `9:16`).
      const [frame, format] = name.split("~");
      const [state, at] = frame.split("@");
      const shown = await page.evaluate(
        async ([state, at, format]) => {
          const { scaena } = window;
          const span = at === null ? 0 : (await scaena.timeline(format)).find((slot) => slot.state === state).span;
          const shown = await scaena.show(state, at === null ? undefined : at * span, format);
          // The worker's frame reaches the page's compositor on its next frame or so.
          for (let i = 0; i < 3; i++) await new Promise(requestAnimationFrame);
          return shown;
        },
        [state, at === undefined ? null : Number(at), format?.replace("x", ":")],
      );
      ms.push(shown.ms);
      const png = await page.locator("#stage").screenshot();
      await writeFile(join(dir, `${name}.png`), png);
      const { width, height, rgba } = decode(png);
      let opaque = 0;
      const colors = new Set();
      for (let i = 0; i < rgba.length; i += 4) {
        opaque += rgba[i + 3] === 255;
        if (colors.size < 2) colors.add(rgba.readUInt32BE(i));
      }
      const fault =
        width !== shown.size[0] || height !== shown.size[1]
          ? `${width} × ${height} px on the page, from a ${shown.size.join(" × ")} canvas`
          : opaque < width * height
            ? `${width * height - opaque} px not opaque`
            : colors.size < 2
              ? "one color"
              : "";
      console.log(`${fault ? "FAIL" : "ok  "} ${painter} ${name}: ${width} × ${height}, painted in ${shown.ms.toFixed(1)} ms${fault ? `: ${fault}` : ""}`);
      if (fault) failures.push(`${painter} ${name}: ${fault}`);
    }
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

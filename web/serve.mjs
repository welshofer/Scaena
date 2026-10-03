// What the web checks share: the repository served from its root, so the built player at
// /web/dist/ reads bundles at their paths in the repository; headless Chromium with WebGPU;
// and the golden frames shown on a page, screenshotted for the parity harness. Needs
// Playwright and its Chromium, as crates/scaena-wasm/www/smoke.mjs does; `playwright`
// resolves as `require` does, so a global install works with NODE_PATH.
import { createServer } from "node:http";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { extname, join, normalize } from "node:path";
import { inflateSync } from "node:zlib";

const types = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm", ".json": "application/json" };

/** The repository at `root`, served on a free port: `{ origin, close }`. `at` is the path it
 * is served at, ending in `/` (`/` by default), and `more` names the media types of other
 * extensions. */
export async function serve(root = process.cwd(), { at = "/", more = {} } = {}) {
  const known = { ...types, ...more };
  const server = createServer(async (req, res) => {
    const asked = decodeURIComponent(new URL(req.url, "http://x").pathname);
    if (!asked.startsWith(at)) return res.writeHead(404).end();
    let path = normalize(join(root, asked.slice(at.length - 1)));
    if (!path.startsWith(root)) return res.writeHead(403).end();
    if (path.endsWith("/")) path = join(path, "index.html");
    try {
      const body = await readFile(path);
      res.writeHead(200, { "content-type": known[extname(path)] ?? "application/octet-stream" }).end(body);
    } catch {
      res.writeHead(404).end();
    }
  }).listen(0, "127.0.0.1");
  await new Promise((ok) => server.once("listening", ok));
  return { origin: `http://127.0.0.1:${server.address().port}`, close: () => server.close() };
}

/** Headless Chromium with WebGPU, which leaves WebGPU canvases blank unless its compositor
 * runs on Vulkan. */
export function launch() {
  const { chromium } = createRequire(import.meta.url)("playwright");
  return chromium.launch({
    args: ["--enable-unsafe-webgpu", "--enable-features=Vulkan", "--use-vulkan=swiftshader", "--use-angle=swiftshader", "--enable-unsafe-swiftshader"],
  });
}

/** A PNG's pixels as RGBA: 8-bit RGB or RGBA, not interlaced, as Chromium writes them. */
export function decode(png) {
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

/** The frames the golden rasters hold, by name (tests/golden/torture/raw.fnv1a): `state` at
 * rest, or `state@fraction`, that fraction of the cue into it; after `~`, in one of the deck's
 * formats (`9x16` for `9:16`). */
export const goldenFrames = async () =>
  (await readFile("tests/golden/torture/raw.fnv1a", "utf8")).trim().split("\n").map((l) => l.split(" ")[0]);

/** Show each of `frames` (`goldenFrames`' names) on `page`, a player's (`window.scaena`) with
 * the torture deck open, and screenshot its stage into `dir/<frame>.png`. Each must be the
 * canvas's size, opaque, and more than one color; what is not goes into `failures`, after
 * `label`. Returns how long each frame took the worker to paint, ms. */
export async function shoot(page, frames, dir, label, failures) {
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
    console.log(`${fault ? "FAIL" : "ok  "} ${label} ${name}: ${width} × ${height}, painted in ${shown.ms.toFixed(1)} ms${fault ? `: ${fault}` : ""}`);
    if (fault) failures.push(`${label} ${name}: ${fault}`);
  }
  return ms;
}

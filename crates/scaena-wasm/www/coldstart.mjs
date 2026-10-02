// SPEC §15 cold start (PLAN 0.14): WASM load → first frame at 1080p, in headless Chromium.
//
//   node crates/scaena-wasm/www/coldstart.mjs [bundle] [runs]     (after `just wasm`; from the repo root)
//
// Opens the page on `bundle` (default tests/bench/b1.scaena) in a fresh browser context per
// run, so nothing is cached, and reads the page's own clock: the first frame painted, ms after
// navigation started. That covers fetching and compiling the module, fetching the bundle and
// its fonts, WebGPU and vello pipeline setup, layout, and paint. Prints each run and the median.
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { extname, join, normalize } from "node:path";

const { chromium } = createRequire(import.meta.url)("playwright");

const root = process.cwd();
const bundle = process.argv[2] ?? "tests/bench/b1.scaena";
const runs = Number(process.argv[3] ?? 5);
const types = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm", ".json": "application/json" };
const server = createServer(async (req, res) => {
  let path = normalize(join(root, decodeURIComponent(new URL(req.url, "http://x").pathname)));
  if (!path.startsWith(root)) return res.writeHead(403).end();
  if (path.endsWith("/")) path = join(path, "index.html");
  try {
    const body = await readFile(path);
    res.writeHead(200, { "content-type": types[extname(path)] ?? "application/octet-stream" }).end(body);
  } catch {
    res.writeHead(404).end();
  }
}).listen(0, "127.0.0.1");
await new Promise((ok) => server.once("listening", ok));
const url = `http://127.0.0.1:${server.address().port}/crates/scaena-wasm/www/?bundle=/${bundle}`;

// The smoke check's flags: headless Chromium paints WebGPU canvases only on Vulkan (SwiftShader here).
const browser = await chromium.launch({
  args: ["--enable-unsafe-webgpu", "--enable-features=Vulkan", "--use-vulkan=swiftshader", "--use-angle=swiftshader", "--enable-unsafe-swiftshader"],
});
const times = [];
let adapter;
try {
  for (let i = 0; i < runs; i++) {
    const context = await browser.newContext({ viewport: { width: 1920, height: 1200 }, deviceScaleFactor: 1 });
    const page = await context.newPage();
    await page.goto(url);
    await page.waitForFunction(() => window.scaena || document.getElementById("status").textContent.startsWith("error"), null, { timeout: 120000 });
    const result = await page.evaluate(() => window.scaena && [window.scaena.firstFrameMs, window.scaena.canvas.adapter]);
    if (!result) throw new Error(await page.textContent("#status"));
    [, adapter] = result;
    times.push(result[0]);
    console.log(`run ${i + 1}: first frame ${result[0].toFixed(0)} ms after navigation`);
    await context.close();
  }
} finally {
  await browser.close();
  server.close();
}
const sorted = [...times].sort((a, b) => a - b);
console.log(`${bundle}: median ${sorted[Math.floor(sorted.length / 2)].toFixed(0)} ms over ${runs} cold starts (${adapter})`);

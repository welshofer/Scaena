// PLAN 0.8 smoke check: the WASM engine in headless Chromium, painting with vello on WebGPU.
//
//   node crates/scaena-wasm/www/smoke.mjs [out-dir]     (after `just wasm`; run from the repo root)
//
// 1. Every state's display list, built in WASM, hashes (FNV-1a over postcard bytes) to the
//    native digest in tests/golden/torture/raw.fnv1a: the engine is bit-identical on wasm32.
// 2. Every one of those states paints through WebGPU: read back from the canvas, the frame is
//    opaque and has ink on it. Each frame is saved as <out-dir>/<state>.png (default
//    target/wasm-smoke) for review and the parity harness (PLAN 0.9).
// 3. The editor's module leaves out the code scanners' word tables (PLAN 2.132): run in Node and
//    handed none, a code text's frame is an error that names the file it needs, and so is one
//    handed bytes that are not that file; handed the repository's, it draws, the digest check 1
//    held the page's frame to.
// Exits 1 on any mismatch or error. Needs Playwright and its Chromium; `playwright` resolves
// like `require` does, so a global install works with NODE_PATH. Headless Chromium hands out a
// WebGPU adapter by default but leaves WebGPU canvases blank unless its compositor runs on
// Vulkan, hence the SwiftShader Vulkan flags below.
import { readFileSync } from "node:fs";
import { createServer } from "node:http";
import { readFile, mkdir, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { extname, join, normalize, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const { chromium } = createRequire(import.meta.url)("playwright");

const root = process.cwd();
const out = process.argv[2] ?? "target/wasm-smoke";
const types = { ".html": "text/html", ".js": "text/javascript", ".mjs": "text/javascript", ".wasm": "application/wasm", ".json": "application/json" };
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
const url = `http://127.0.0.1:${server.address().port}/crates/scaena-wasm/www/`;

const expected = (await readFile("tests/golden/torture/raw.fnv1a", "utf8")).trim().split("\n").map((l) => l.split(" "));
const browser = await chromium.launch({
  args: [
    "--enable-unsafe-webgpu",
    "--enable-features=Vulkan",
    "--use-vulkan=swiftshader",
    "--use-angle=swiftshader",
    "--enable-unsafe-swiftshader",
  ],
});
const failures = [];
try {
  const page = await browser.newPage({ viewport: { width: 1920, height: 1200 }, deviceScaleFactor: 1 });
  page.on("console", (m) => m.type() === "error" && failures.push(`console: ${m.text()}`));
  await page.goto(url);
  await page.waitForFunction(() => window.scaena || document.getElementById("status").textContent.startsWith("error"), null, { timeout: 120000 });
  const adapter = await page.evaluate(() => window.scaena?.canvas.adapter);
  if (!adapter) throw new Error(await page.textContent("#status"));
  const firstFrame = await page.evaluate(() => window.scaena.firstFrameMs);
  console.log(`adapter: ${adapter}; first frame ${firstFrame.toFixed(0)} ms after navigation (fetch, compile, fonts, WebGPU, paint)`);
  const frameMs = [];
  await mkdir(out, { recursive: true });
  for (const [name, digest] of expected) {
    // `state` at rest, or `state@fraction`: that fraction of the transition into it; after
    // `~`, in one of the deck's formats (`9x16` for `9:16`), else on its own canvas.
    const [frame, format] = name.split("~");
    const [state, at] = frame.split("@");
    await page.evaluate((f) => window.scaena.setFormat(f), format?.replace("x", ":") ?? "");
    const [got, ms] = await page.evaluate(([s, at]) => {
      const player = window.scaena.player;
      const t = at === undefined ? Infinity : Number(at) * player.duration(s);
      const start = performance.now();
      const bytes = player.frame(s, t);
      const ms = performance.now() - start;
      let h = 0xcbf29ce484222325n;
      for (const b of bytes) h = BigInt.asUintN(64, (h ^ BigInt(b)) * 0x100000001b3n);
      return [h.toString(16).padStart(16, "0"), ms];
    }, [state, at]);
    frameMs.push(ms);
    // Paint, then read the canvas back in the same task, before the browser presents it.
    const shot = await page.evaluate(([s, at]) => {
      window.scaena.select.value = s;
      window.scaena.show(at === undefined ? Infinity : Number(at) * window.scaena.player.duration(s));
      const stage = document.getElementById("stage");
      const copy = Object.assign(document.createElement("canvas"), { width: stage.width, height: stage.height });
      const ctx = copy.getContext("2d", { willReadFrequently: true });
      ctx.drawImage(stage, 0, 0);
      const px = ctx.getImageData(0, 0, copy.width, copy.height).data;
      let opaque = 0, ink = 0;
      for (let i = 0; i < px.length; i += 4) {
        opaque += px[i + 3] === 255;
        ink += px[i] + px[i + 1] + px[i + 2] < 3 * 128;
      }
      const status = document.getElementById("status").textContent;
      return { status, opaque, ink, pixels: px.length / 4, png: copy.toDataURL("image/png") };
    }, [state, at]);
    await writeFile(join(out, `${name}.png`), Buffer.from(shot.png.split(",")[1], "base64"));
    const painted = shot.status.startsWith(`${state} · `) && shot.opaque === shot.pixels && shot.ink > 0;
    const ok = got === digest && painted;
    console.log(
      `${ok ? "ok  " : "FAIL"} ${name}: digest ${got}${got === digest ? "" : ` (native ${digest})`}; ` +
        `${shot.status}; ${shot.ink} ink px, ${shot.pixels - shot.opaque} not opaque; frame() ${ms.toFixed(1)} ms`,
    );
    if (!ok) failures.push(name);
  }
  frameMs.sort((a, b) => a - b);
  console.log(`frame() in WASM: min ${frameMs[0].toFixed(1)}, median ${frameMs[frameMs.length >> 1].toFixed(1)}, max ${frameMs.at(-1).toFixed(1)} ms`);
} catch (e) {
  failures.push(String(e));
} finally {
  await browser.close();
  server.close();
}
try {
  const pkg = "crates/scaena-wasm/www/pkg";
  const glue = await import(pathToFileURL(resolve(pkg, "scaena_wasm.js")).href);
  await glue.default({ module_or_path: await readFile(join(pkg, "scaena_wasm_bg.wasm")) });
  glue.setHyphenation((code) => readFileSync(`crates/scaena-engine/hyphenation/${code}.bin`));
  const bundle = "tests/fixtures/torture.scaena";
  const deckJson = await readFile(join(bundle, "deck.json"), "utf8");
  const deck = JSON.parse(deckJson);
  const player = new glue.Player(deckJson, await readFile(join(bundle, deck.theme), "utf8"));
  const sources = Object.values(deck.data ?? {}).flatMap(({ source }) => (typeof source === "string" ? [source] : []));
  for (const path of [...deck.fonts.map((f) => f.file), ...sources, ...player.imageFiles()])
    player.addFile(path, await readFile(join(bundle, path)));
  const tables = (name) => readFileSync(`crates/scaena-core/code/${name}.txt`);
  const said = (loader) => {
    glue.setCodeTables(loader);
    try {
      player.frame("code", Infinity);
      return "";
    } catch (e) {
      return String(e?.message ?? e);
    }
  };
  const none = said(() => undefined);
  const wrong = said((name) => Buffer.concat([tables(name), Buffer.from("\n")]));
  const handed = said(tables);
  const ok = none.includes("`clike`") && none.includes("not handed over") && wrong.includes("not the engine's tables") && !handed;
  console.log(`${ok ? "ok  " : "FAIL"} code tables: none: ${none}; another file: ${wrong}; the repository's: ${handed || "drawn"}`);
  if (!ok) failures.push("code tables");
} catch (e) {
  failures.push(`code tables: ${e}`);
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : `all ${expected.length} states: WASM display lists match native, WebGPU painted each`);
process.exit(failures.length ? 1 : 0);

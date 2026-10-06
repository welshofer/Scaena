// PLAN 2.5 check: single-file HTML exports, opened from their addresses on disk in headless
// Chromium with the network off, as a file opens from a USB stick.
//
//   node web/standalone.mjs [out-dir]     (after `just web`; from the repository's root)
//
// `scaena export --format html` (run here by cargo, so `scaena` is built after the web
// player it carries) writes the torture deck, the revenue example, and the example's `mix`
// and `intro` alone into <out-dir>/standalone/ (default out-dir target/web-smoke). Then:
// - The engine the file carries (the player's module alone) paints every frame the golden
//   rasters hold byte for byte as the web player's engine (the editor's module) does, run in
//   Node: the pixels each makes, before a browser shows them. The one has the hyphenation
//   patterns compiled in, and the other is handed them (ADR-0015).
// - The torture deck's file shows every golden frame, painted by the CPU painter in the file's
//   own worker, saved as <out-dir>/standalone-cpu/<frame>.png, where the parity harness holds
//   them to the goldens (SPEC §13.5; `just web-smoke` runs it). A browser's screenshots of one
//   frame can differ by a step in a few pixels between a page from a server and one from a
//   file, so these are held to the goldens, not to the player's screenshots.
// - The revenue example's file plays: on, a hold that goes on by itself, End, Home. The
//   state shown reads in its live region as the export wrote it, and what reads as it did
//   stays put. The canvas is hidden from a screen reader.
// - The file of `mix` and `intro` plays those two, in that order.
// - WebGPU paints a file, and the presenter view opens from one and follows it.
// - Nothing any file does asks for more than itself and its worker's blob, and no page logs
//   an error. The page's content security policy refuses a fetch the page tries.
// Exits 1 on any failure.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { mkdir, stat } from "node:fs/promises";
import { join, relative, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { decode, goldenFrames, launch, shoot } from "./serve.mjs";

const out = process.argv[2] ?? "target/web-smoke";
const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};

/** `bundle` exported as one file, `name`.html in <out-dir>/standalone, with `args`: its address
 * and its size. */
async function exported(bundle, name, ...args) {
  const file = join(out, "standalone", `${name}.html`);
  const scaena = ["run", "-q", "-p", "scaena-cli", "--locked", "--"];
  execFileSync("cargo", [...scaena, "export", bundle, "--format", "html", "--out", file, ...args], { stdio: "inherit" });
  return { url: pathToFileURL(resolve(file)).href, bytes: (await stat(file)).size };
}

/** Each of `frames` (the golden rasters' names) as the module in `dir`, a wasm-bindgen package,
 * paints the torture deck by the CPU painter, in Node: a digest of its pixels, by name. The
 * frames go in the order a page's `shoot` shows them, after the first state at rest. */
async function painted(dir, frames) {
  const bundle = "tests/fixtures/torture.scaena";
  const files = [];
  const walk = (at) => {
    for (const name of readdirSync(at)) statSync(join(at, name)).isDirectory() ? walk(join(at, name)) : files.push(relative(bundle, join(at, name)));
  };
  walk(bundle);
  const glue = await import(pathToFileURL(resolve(dir, "scaena_wasm.js")).href);
  await glue.default({ module_or_path: readFileSync(join(dir, "scaena_wasm_bg.wasm")) });
  // The editor's module is handed the hyphenation patterns it leaves out, here from the
  // repository, as its page hands them over (ADR-0015); the player's has them compiled in.
  glue.setHyphenation((code) => readFileSync(`crates/scaena-engine/hyphenation/${code}.bin`));
  const deck = readFileSync(join(bundle, "deck.json"), "utf8");
  const theme = JSON.parse(deck).theme;
  const player = new glue.Player(deck, readFileSync(join(bundle, theme), "utf8"));
  for (const path of files) if (path !== "deck.json" && path !== theme) player.addFile(path, readFileSync(join(bundle, path)));
  const width = () => Math.round(player.canvasSize()[0]);
  player.pixels(JSON.parse(player.timeline())[0].state, Infinity, width());
  const digests = new Map();
  for (const name of frames) {
    const [frame, format] = name.split("~");
    const [state, at] = frame.split("@");
    player.setFormat(format?.replace("x", ":"));
    const span = JSON.parse(player.timeline()).find((slot) => slot.state === state).span;
    const pixels = player.pixels(state, at === undefined ? Infinity : Number(at) * span, width());
    digests.set(name, createHash("sha256").update(pixels).digest("hex"));
  }
  player.free();
  return digests;
}

const frames = await goldenFrames();
const [own, players] = await Promise.all(
  ["crates/scaena-wasm/player", "crates/scaena-wasm/www/pkg"].map((dir) => painted(dir, frames)),
);
const unlike = frames.filter((name) => own.get(name) !== players.get(name));
check(
  unlike.length === 0,
  `the engine a file carries paints all ${frames.length} golden frames byte for byte as the web player's does${unlike.length ? `; not ${unlike.join(", ")}` : ""}`,
);

await mkdir(join(out, "standalone"), { recursive: true });
const torture = await exported("tests/fixtures/torture.scaena", "torture");
const revenue = await exported("docs/examples/revenue.deck.json", "revenue");
const picked = await exported("docs/examples/revenue.deck.json", "picked", "--states", "mix,intro");
const mb = (bytes) => `${(bytes / 1e6).toFixed(2)} MB`;
console.log(`exported: the torture deck ${mb(torture.bytes)}, the revenue example ${mb(revenue.bytes)}`);

const browser = await launch();
try {
  // Tall enough for the torture deck's 9:16 format at its own size, under the controls.
  const context = await browser.newContext({ viewport: { width: 1920, height: 2000 }, deviceScaleFactor: 1 });
  await context.setOffline(true);
  const requested = [];
  context.on("request", (r) => requested.push(r.url()));
  const watch = (page, label) => {
    page.on("console", (m) => m.type() === "error" && failures.push(`${label}: console: ${m.text()}`));
    page.on("pageerror", (e) => failures.push(`${label}: ${e.message}`));
  };
  /** `page` once its player has opened the file and shown its first frame. */
  const ready = async (page) => {
    await page.waitForFunction(
      () => window.scaena?.at || document.querySelector("#status")?.textContent.startsWith("error"),
      null,
      { timeout: 120000 },
    );
    const status = await page.evaluate(() => (window.scaena?.at ? "" : document.querySelector("#status").textContent));
    if (status) throw new Error(status);
  };
  const reading = (page) => page.evaluate(() => document.querySelector("#reading").innerHTML);
  const template = (page, state) =>
    page.evaluate((state) => document.querySelector(`template[data-state="${state}"]`)?.innerHTML, state);

  // The torture deck: every golden frame, by the CPU painter.
  const tortured = await context.newPage();
  watch(tortured, "torture");
  const navigated = performance.now();
  await tortured.goto(`${torture.url}?painter=cpu`);
  await ready(tortured);
  console.log(`the torture deck's file shows its first frame ${(performance.now() - navigated).toFixed(0)} ms after navigation`);
  const ms = await shoot(tortured, frames, join(out, "standalone-cpu"), "standalone cpu", failures);
  ms.sort((a, b) => a - b);
  console.log(`${frames.length} frames painted in ${ms[0].toFixed(1)}–${ms.at(-1).toFixed(1)} ms, median ${ms[ms.length >> 1].toFixed(1)}`);
  await tortured.close();

  // The revenue example: it plays, and reads as it plays.
  const page = await context.newPage();
  watch(page, "revenue");
  await page.goto(`${revenue.url}?painter=cpu`);
  await ready(page);
  const at = () => page.evaluate(() => window.scaena.at());
  check(
    JSON.stringify(await page.evaluate(() => window.scaena.states)) === '["intro","revenue","mix","close"]',
    "the revenue example's file plays its four states",
  );
  check((await page.title()) === "Q3 Review" && (await page.getAttribute("html", "lang")) === "en-US", "it has the deck's title and language");
  check((await page.getAttribute("#stage", "aria-hidden")) === "true" && (await page.getAttribute("#reading", "aria-live")) === "polite", "the canvas is hidden from a screen reader, and the reading is a live region");
  const intro = await reading(page);
  check(intro === (await template(page, "intro")) && intro.includes("<h1 data-node=\"title\">Q3 Review</h1>"), `the first state reads as the export wrote it: ${intro}`);

  await page.keyboard.press("ArrowRight");
  await page.waitForFunction(() => window.scaena.at().index === 1, null, { timeout: 30000 });
  const shown = await reading(page);
  check(shown === (await template(page, "revenue")), `on: the next state reads: ${shown}`);
  check(shown.includes('aria-label="Quarterly revenue by product, Q4 2025 through Q3 2026."'), "its chart reads by its alt text");
  // What reads as it did stays put; only the title changes on the way to `mix`. (Each element
  // is marked by a property: an attribute would change what it reads.)
  await page.evaluate(() => {
    for (const el of document.querySelectorAll("#reading > *")) el.was = true;
  });
  await page.waitForFunction(() => window.scaena.at().index === 2, null, { timeout: 30000 });
  const kept = await page.evaluate(() => [...document.querySelectorAll("#reading > *")].map((el) => [el.dataset.node, el.was === true]));
  check(JSON.stringify(kept) === '[["title",false],["rev",true],["note",true]]', `a hold goes on by itself, and only what changed reads anew: ${JSON.stringify(kept)}`);
  await page.keyboard.press("End");
  await page.waitForFunction(() => window.scaena.at().index === 3, null, { timeout: 30000 });
  check((await reading(page)) === '<h1 data-node="title">Thank you</h1>', "End: the last state, and its reading");
  await page.keyboard.press("Home");
  await page.waitForFunction(() => window.scaena.at().index === 0, null, { timeout: 30000 });
  check((await reading(page)) === intro, "Home: the first again");

  // The presenter view, from the file.
  const [presenter] = await Promise.all([context.waitForEvent("page"), page.keyboard.press("p")]);
  watch(presenter, "presenter");
  await presenter.waitForFunction(() => window.scaena?.follows?.()?.startsWith("1 / 4"), null, { timeout: 120000 });
  await page.keyboard.press("ArrowRight");
  await presenter.waitForFunction(() => window.scaena.follows().startsWith("2 / 4"), null, { timeout: 30000 }).catch(() => {});
  check((await presenter.evaluate(() => window.scaena.follows())).startsWith("2 / 4 · revenue"), "the presenter view opens from the file and follows it");
  await presenter.close();
  await page.close();

  // `--states mix,intro`: those two, in that order.
  const two = await context.newPage();
  watch(two, "picked");
  await two.goto(`${picked.url}?painter=cpu`);
  await ready(two);
  check(JSON.stringify(await two.evaluate(() => window.scaena.states)) === '["mix","intro"]', "a file of `mix` and `intro` plays those, in that order");
  check((await reading(two)).includes("…and the mix shifted"), "and opens on `mix`, reading it");
  await two.close();

  // The policy, not only the network being off, refuses what the page would fetch. (This page
  // is not watched: the refusal logs an error.)
  const policed = await context.newPage();
  await policed.goto(`${picked.url}?painter=cpu`);
  await ready(policed);
  const refused = await policed.evaluate(async () => {
    const violated = new Promise((ok) =>
      document.addEventListener("securitypolicyviolation", (e) => ok(e.effectiveDirective), { once: true }),
    );
    await fetch("data:text/plain,hello").catch(() => {});
    return Promise.race([violated, new Promise((ok) => setTimeout(() => ok("nothing"), 5000))]);
  });
  check(refused === "connect-src", `its content security policy refuses a fetch: ${refused}`);
  await policed.close();

  // WebGPU paints one.
  const gpu = await context.newPage();
  watch(gpu, "webgpu");
  await gpu.goto(`${revenue.url}?painter=gpu`);
  await ready(gpu);
  const painter = await gpu.evaluate(() => window.scaena.painter);
  await gpu.evaluate(() => new Promise((ok) => requestAnimationFrame(() => requestAnimationFrame(ok))));
  const { rgba } = decode(await gpu.locator("#stage").screenshot());
  const colors = new Set();
  for (let i = 0; i < rgba.length && colors.size < 2; i += 4) colors.add(rgba.readUInt32BE(i));
  check(painter === "webgpu" && colors.size > 1, `WebGPU paints a file: ${painter}`);
  await gpu.close();

  const elsewhere = requested.filter((url) => !url.startsWith("file:") && !url.startsWith("blob:"));
  check(elsewhere.length === 0, `nothing is asked of the network: ${requested.length} requests, ${elsewhere.length} beyond the files and their workers${elsewhere.length ? `: ${elsewhere.slice(0, 5)}` : ""}`);
} catch (e) {
  failures.push(String(e));
} finally {
  await browser.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "single files play offline, read as they play, and paint as the player does");
process.exit(failures.length ? 1 : 0);

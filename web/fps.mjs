// Gate 2, criterion 1: how a deck plays in a browser, state by state, by the player's own frame
// meter (`?fps`). It serves the repository, opens the player (built by `just web`) in the
// browser asked for, plays each state's cue and then its hold, as a presentation does, and
// prints what the meter read for each: frames a second, the worst frame, the frames that came
// late for a 60 Hz display, and the mean paint. Each is held to the bar docs/gate-2.md sets:
// on WebGPU, 58 frames a second and no more than 2 late on every cue; on the CPU painter, 58 on
// a cue that draws no shader and 30 on one that does.
//
// Run it on the machine whose browsers count, with a window, so the browser paints with that
// machine's GPU:
//
//     NODE_PATH=$(npm root -g) node web/fps.mjs --browser chrome                   # WebGPU
//     NODE_PATH=$(npm root -g) node web/fps.mjs --browser firefox --painter cpu    # the fallback
//
// `chrome` is the Chrome installed on the machine. `chromium`, `firefox`, and `webkit` are
// Playwright's own (`npx playwright install firefox`). Playwright's WebKit is not Safari, so
// docs/gate-2.md has Safari's steps by hand. Options: `--bundle PATH` (the trails example by
// default), `--painter gpu|cpu` (the player's choice by default: WebGPU where there is an
// adapter), `--states a,b`, `--helpers N` (the most workers the CPU painter shares a shader's
// rows with, PLAN 2.28; 0 for none), and `--headless`, which in Chromium paints WebGPU on
// SwiftShader.
import { existsSync, readFileSync, statSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { launch, serve } from "./serve.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const option = (name, fallback) => {
  const i = args.indexOf(`--${name}`);
  return i < 0 ? fallback : args[i + 1];
};
const browserName = option("browser", "chromium");
const bundle = option("bundle", "docs/examples/trails.deck.json");
const painter = option("painter", undefined);
const only = option("states", undefined)?.split(",");
const helpers = option("helpers", undefined);
const headless = args.includes("--headless");
/** The states that draw a shader: each shows a shader node. */
const shaded = (() => {
  const path = join(root, bundle);
  const deck = JSON.parse(readFileSync(statSync(path).isDirectory() ? join(path, "deck.json") : path, "utf8"));
  const isShader = (id) => deck.nodes?.[id]?.type === "shader";
  return new Set((deck.states ?? []).filter((s) => Object.keys(s.props ?? {}).some(isShader)).map((s) => s.id));
})();
if (!existsSync(join(root, "web/dist/index.html"))) {
  console.error("No player to open: build it with `just web` first.");
  process.exit(2);
}

/** The browser, with a window unless `--headless`. */
async function open() {
  if (browserName === "chromium" && headless) return launch();
  const playwright = createRequire(import.meta.url)("playwright");
  const kind = { chrome: "chromium", chromium: "chromium", firefox: "firefox", webkit: "webkit" }[browserName];
  if (!kind) throw new Error(`--browser ${browserName}: chrome, chromium, firefox, or webkit`);
  return playwright[kind].launch({
    headless,
    channel: browserName === "chrome" ? "chrome" : undefined,
    args: kind === "chromium" ? ["--enable-unsafe-webgpu"] : [],
  });
}

const server = await serve(root);
const browser = await open();
const reading = /^([\d.]+) fps · worst (\d+) ms · (\d+) late of (\d+) · paint ([\d.]+) ms$/;
try {
  const page = await browser.newPage({ viewport: { width: 1920, height: 1080 } });
  const query = new URLSearchParams({ bundle: `/${bundle}`, fps: "" });
  if (painter) query.set("painter", painter);
  if (helpers !== undefined) query.set("helpers", helpers);
  await page.goto(`${server.origin}/web/dist/?${query}`);
  await page.waitForFunction(() => window.scaena, null, { timeout: 120000 });
  const slots = await page.evaluate(() => window.scaena.timeline());
  const paints = await page.evaluate(() => (window.scaena.opened().painter === "webgpu" ? "WebGPU" : "CPU painter"));
  console.log(`${browserName}${headless ? ", headless" : ""} · ${await browser.version()} · ${paints} · ${bundle}`);
  console.log(`${"state".padEnd(16)} ${"cue".padStart(6)} ${"hold".padStart(6)}   reading`);
  const rows = [];
  for (const [k, slot] of slots.entries()) {
    if (only ? !only.includes(slot.state) : !(slot.span > 0)) continue;
    // A run the meter did not see leaves the last run's reading: cleared, it says so.
    await page.evaluate(() => (document.querySelector("#meter").value = ""));
    await page.evaluate((k) => window.scaena.run(k, 0), k);
    await page.waitForFunction(() => window.scaena.at().playing, null, { timeout: 5000, polling: 5 }).catch(() => {});
    // The cue, then its hold: until the state comes to rest, or its hold is all but over and
    // the deck would go on, which a frame or two of the next state's cue may have done.
    const end = slot.span + slot.hold - 50;
    await page.waitForFunction(
      ([k, end]) => {
        const at = window.scaena.at();
        return !at.playing || at.index !== k || at.t >= end;
      },
      [k, end],
      { timeout: slot.span + slot.hold + 30000, polling: 20 },
    );
    await page.evaluate((k) => window.scaena.seek(k), k);
    const text = (await page.locator("#meter").textContent()) ?? "";
    const [, fps, worst, late, frames, paint] = (reading.exec(text) ?? []).map(Number);
    // The bar (docs/gate-2.md): 58 frames a second, the meter's reading of 60 Hz, and on WebGPU
    // no more than 2 late; on the CPU painter, 30 where the state draws a shader.
    const bar = paints === "CPU painter" && shaded.has(slot.state) ? 30 : 58;
    const met = fps >= bar && (paints !== "WebGPU" || late <= 2);
    rows.push({ state: slot.state, fps, worst, late, frames, paint, bar, met });
    const ms = (n) => `${Math.round(n)} ms`.padStart(6);
    const verdict = `${met ? "meets" : "short of"} ${bar} fps${shaded.has(slot.state) ? ", a shader" : ""}`;
    console.log(`${slot.state.padEnd(16)} ${ms(slot.span)} ${ms(slot.hold)}   ${frames ? `${text} · ${verdict}` : "no frames: it holds still"}`);
  }
  const measured = rows.filter((r) => r.frames);
  if (measured.length) {
    const low = measured.reduce((a, b) => (b.fps < a.fps ? b : a));
    const worst = measured.reduce((a, b) => (b.worst > a.worst ? b : a));
    const late = measured.reduce((n, r) => n + r.late, 0);
    const all = measured.reduce((n, r) => n + r.frames, 0);
    const short = measured.filter((r) => !r.met).map((r) => r.state);
    console.log(
      `lowest ${low.fps} fps (${low.state}) · worst frame ${worst.worst} ms (${worst.state}) · ${late} late of ${all}` +
        ` · the bar for ${paints}: ${short.length ? `short on ${short.length} of ${measured.length} cues (${short.join(", ")})` : `met on all ${measured.length} cues`}`,
    );
  }
} finally {
  await browser.close();
  server.close();
}

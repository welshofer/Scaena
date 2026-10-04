// Gate 2, criterion 1: how a deck plays in a browser, state by state, by the player's own frame
// meter (`?fps`). It serves the repository, opens the player (built by `just web`) in the
// browser asked for, plays each state's cue and then its hold, as a presentation does, and
// prints what the meter read for each: frames a second, the worst frame, the frames that came
// late for a 60 Hz display, and the mean paint. Then the lowest of them, beside the bar
// docs/gate-2.md proposes.
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
// adapter), `--states a,b`, and `--headless`, which in Chromium paints WebGPU on SwiftShader.
import { existsSync } from "node:fs";
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
const headless = args.includes("--headless");
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
    rows.push({ state: slot.state, fps, worst, late, frames, paint });
    const ms = (n) => `${Math.round(n)} ms`.padStart(6);
    console.log(`${slot.state.padEnd(16)} ${ms(slot.span)} ${ms(slot.hold)}   ${frames ? text : "no frames: it holds still"}`);
  }
  const measured = rows.filter((r) => r.frames);
  if (measured.length) {
    const low = measured.reduce((a, b) => (b.fps < a.fps ? b : a));
    const worst = measured.reduce((a, b) => (b.worst > a.worst ? b : a));
    const late = measured.reduce((n, r) => n + r.late, 0);
    const all = measured.reduce((n, r) => n + r.frames, 0);
    const bar = paints === "WebGPU" ? 55 : 30;
    console.log(
      `lowest ${low.fps} fps (${low.state}) · worst frame ${worst.worst} ms (${worst.state}) · ${late} late of ${all}` +
        ` · the proposed bar for ${paints} is ${bar} fps: ${low.fps >= bar ? "met" : "not met"}`,
    );
  }
} finally {
  await browser.close();
  server.close();
}

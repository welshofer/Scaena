// PLAN 2.73 check: the static site `just site` builds (target/site), installed and then used with
// no network, in headless Chromium, served from a path under its origin as web/site.mjs serves it.
//
//   node web/offline.mjs     (after `just site`; from the repository's root)
//
// - Opened once, the site registers its service worker, which keeps every file the build wrote; the
//   pages say "Works offline", and name the web app manifest a browser installs the site by.
// - The network gone: the player opens again at the site's address and plays every state of the
//   demo deck, each painting a frame that is not blank; the editor opens on it, compiles and lints
//   it, and downloads it, the subsetter loaded from what the site keeps.
// - Nothing the pages ask is refused, and nothing is asked of another origin.
// Exits 1 on any failure.
import { readdir, readFile } from "node:fs/promises";
import { join } from "node:path";
import { decode, launch, serve } from "./serve.mjs";

const root = join(process.cwd(), "target/site");
const at = "/scaena/demo/";
const media = { ".css": "text/css", ".png": "image/png", ".ttf": "font/ttf", ".csv": "text/csv", ".txt": "text/plain", ".md": "text/markdown", ".svg": "image/svg+xml", ".webmanifest": "application/manifest+json" };
const site = await serve(root, { at, more: media });
const home = `${site.origin}${at}`;
const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};

const decks = await readdir(join(root, "decks"));
const deck = JSON.parse(await readFile(join(root, "decks", decks[0], "deck.json"), "utf8"));
const states = deck.states.map((s) => s.id);
const built = (await readdir(join(root, "assets"))).map((f) => `assets/${f}`);

const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1920, height: 1200 }, deviceScaleFactor: 1, acceptDownloads: true });
  const outside = [];
  const refused = [];
  context.on("request", (r) => !r.url().startsWith(home) && !/^(blob|data):/.test(r.url()) && outside.push(r.url()));
  context.on("requestfailed", (r) => refused.push(`${r.url()}: ${r.failure()?.errorText}`));
  const watch = (page) => {
    page.on("pageerror", (e) => failures.push(`${page.url()}: ${e.message}`));
    page.on("console", (m) => m.type() === "error" && failures.push(`${page.url()}: console: ${m.text()}`));
    return page;
  };
  const opened = async (page, test) => {
    await page.waitForFunction(test, null, { timeout: 120000 }).catch(() => {});
    return page.evaluate(() => (document.querySelector("#status")?.textContent ?? "").startsWith("error") && document.querySelector("#status").textContent);
  };

  // Online, once: the player, then the editor, each opening the demo deck.
  const page = watch(await context.newPage());
  await page.goto(`${home}?painter=cpu`);
  check(!(await opened(page, () => window.scaena?.at)), "the player opens the demo deck");
  await page.waitForFunction(() => document.documentElement.dataset.offline === "ready", null, { timeout: 120000 }).catch(() => {});
  check(await page.isVisible("#offline"), `the player says it works offline: ${await page.textContent("#offline")}`);
  const keeps = await page.evaluate(async () => {
    const names = await caches.keys();
    const site = names.find((n) => n.startsWith("scaena-site-"));
    const kept = site ? (await (await caches.open(site)).keys()).map((r) => r.url) : [];
    return { site, kept, controlled: !!navigator.serviceWorker.controller, manifest: document.querySelector('link[rel="manifest"]')?.href };
  });
  const missing = built.filter((f) => !keeps.kept.includes(`${home}${f}`));
  check(keeps.site && !missing.length, `its service worker keeps every file the build wrote (${keeps.kept.length})${missing.length ? `; not: ${missing.join(", ")}` : ""}`);
  check(keeps.controlled, "and answers the page");
  const manifest = keeps.manifest ? await (await fetch(keeps.manifest)).json().catch(() => undefined) : undefined;
  check(manifest?.start_url === "./" && manifest?.display === "standalone" && manifest?.icons?.length, `the page names the web app manifest that installs the site: ${keeps.manifest?.slice(site.origin.length)}`);
  await page.goto(`${home}editor.html?painter=cpu`);
  check(!(await opened(page, () => window.scaena?.last?.())), "the editor opens the demo deck");
  await page.waitForFunction(() => document.documentElement.dataset.offline === "ready", null, { timeout: 60000 }).catch(() => {});
  check(await page.isVisible("#offline"), "and says it works offline");

  // The network gone.
  await context.setOffline(true);
  await page.goto(`${home}?painter=cpu`);
  check(!(await opened(page, () => window.scaena?.at)), "offline, the player opens again at the site's address");
  const shown = await page.evaluate(() => window.scaena?.states ?? []);
  check(JSON.stringify(shown) === JSON.stringify(states), `it plays the deck's ${states.length} states`);
  const blank = [];
  for (const state of shown) {
    await page.evaluate((s) => window.scaena.show(s), state);
    const { rgba } = decode(await page.locator("#stage").screenshot());
    const colors = new Set();
    for (let i = 0; i < rgba.length && colors.size <= 16; i += 4 * 97) colors.add(rgba.readUInt32BE(i));
    if (colors.size <= 16) blank.push(state);
  }
  check(shown.length > 0 && !blank.length, `each paints a frame that is not blank${blank.length ? `: ${blank.join(", ")}` : ""}`);

  await page.goto(`${home}editor.html?painter=cpu`);
  check(!(await opened(page, () => window.scaena?.last?.())), "offline, the editor opens on the deck");
  await page.waitForFunction(() => window.scaena.wholes().length > 0 && window.scaena.last().whole, null, { timeout: 60000 }).catch(() => {});
  const linted = await page.evaluate(() => window.scaena.last());
  const errors = linted?.findings.filter((f) => f.severity === "error") ?? [];
  check(linted?.valid && linted.whole && !errors.length, `it compiles the deck and lints every state with no error (${errors.map((f) => f.code).join(", ") || "none"})`);
  const [download] = await Promise.all([page.waitForEvent("download", { timeout: 60000 }), page.click("#download")]).catch(() => [undefined]);
  const zip = download ? await readFile(await download.path()) : Buffer.alloc(0);
  check(zip[0] === 0x50 && zip[1] === 0x4b, `Download works, the subsetter loaded from what the site keeps: ${zip.length} bytes`);

  check(!outside.length, `nothing is asked of another origin${outside.length ? `: ${[...new Set(outside)].join(", ")}` : ""}`);
  check(!refused.length, `nothing the pages ask is refused${refused.length ? `: ${refused.join("; ")}` : ""}`);
} finally {
  await browser.close();
  await site.close();
}

if (failures.length) {
  console.log(`\n${failures.length} failed:\n  ${failures.join("\n  ")}`);
  process.exit(1);
}
console.log("\nall passed");

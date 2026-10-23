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
// - Every theme the registry lists, with its thumbnail and its fonts, is among what the site keeps,
//   and offline the editor makes a deck in each, with its fonts alone, and saves it in the browser
//   (PLAN T.0c). The site's install, every file the build wrote, is recorded.
// - A new build (lift-up REL-2): a newer sw.js served, the editor finds it, its worker fetches
//   every file of the site past the browser's HTTP cache and waits beside the one answering the
//   page, which keeps its build; the page offers "A new version is ready: reload", and pressed, it
//   loads again answered by the new worker alone, the old build's cache gone.
// - Nothing the pages ask is refused, and nothing is asked of another origin.
// Exits 1 on any failure.
import { readdir, readFile, stat } from "node:fs/promises";
import { join } from "node:path";
import { decode, launch, serve } from "./serve.mjs";

const root = join(process.cwd(), "target/site");
const at = "/scaena/demo/";
const media = { ".css": "text/css", ".png": "image/png", ".ttf": "font/ttf", ".csv": "text/csv", ".txt": "text/plain", ".md": "text/markdown", ".svg": "image/svg+xml", ".webmanifest": "application/manifest+json" };
/** Each path the server is asked, and the sw.js it answers with in place of the build's. */
const seen = [];
let newer;
const site = await serve(root, { at, more: media, seen, instead: (asked) => (asked === `${at}sw.js` ? newer : undefined) });
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
// The themes that ship (PLAN T.0c): each by its name, with the fonts its families name.
const registry = JSON.parse(await readFile("docs/themes/registry.json", "utf8")).themes;
const shipped = await Promise.all(
  registry.map(async (t) => {
    const theme = JSON.parse(await readFile(t.files[0], "utf8"));
    const fonts = new Set(Object.values(theme.type.families).flatMap((f) => [f.file, f.italic?.file].filter(Boolean)));
    return { slug: t.slug, name: t.name, fonts: fonts.size };
  }),
);

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
    return page.evaluate(() => (document.querySelector("#status")?.textContent ?? "").startsWith("Error") && document.querySelector("#status").textContent);
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
  const thumbnails = shipped.filter((t) => !built.some((f) => f.startsWith(`assets/${t.slug}-`) && f.endsWith(".png")));
  check(!thumbnails.length, `it keeps each theme's thumbnail, drawn at build${thumbnails.length ? `; not: ${thumbnails.map((t) => t.name)}` : ""}`);
  const files = [...built, ...(await readdir(root)).filter((f) => f !== "assets" && f !== "decks")];
  const sizes = await Promise.all(files.map(async (f) => (await stat(join(root, f))).size));
  console.log(`     the site's install: ${files.length} files, ${sizes.reduce((a, b) => a + b, 0)} bytes, the demo deck aside`);
  const manifest = keeps.manifest ? await (await fetch(keeps.manifest)).json().catch(() => undefined) : undefined;
  check(manifest?.start_url === "./" && manifest?.display === "standalone" && manifest?.icons?.length, `the page names the web app manifest that installs the site: ${keeps.manifest?.slice(site.origin.length)}`);
  await page.goto(`${home}editor.html?painter=cpu`);
  check(!(await opened(page, () => window.scaena?.last?.())), "the editor opens the demo deck");
  await page.waitForFunction(() => document.documentElement.dataset.offline === "ready", null, { timeout: 60000 }).catch(() => {});
  check(await page.isVisible("#offline"), "and says it works offline");

  // A new build (lift-up REL-2): the same files under another version. The editor, told to look,
  // finds it; its worker installs beside the one answering the page, and waits.
  newer = (await readFile(join(root, "sw.js"), "utf8")).replace(/^const version = "[^"]+";$/m, 'const version = "newer";');
  check(newer.includes('"newer"'), "a newer sw.js is served in place of the build's");
  seen.length = 0;
  const was = await page.evaluate(async () => ({ worker: navigator.serviceWorker.controller?.scriptURL, caches: await caches.keys() }));
  await page.evaluate(() => navigator.serviceWorker.getRegistration().then((r) => r.update()));
  await page.waitForFunction(() => !document.querySelector("#update")?.hidden, null, { timeout: 60000 }).catch(() => {});
  check(await page.isVisible("#update"), `the editor offers it: ${await page.textContent("#update")}`);
  const fetched = built.filter((f) => seen.includes(`${at}${f}`));
  check(
    fetched.length === built.length,
    `the new worker fetched every file of the site past the browser's HTTP cache (${fetched.length} of ${built.length})`,
  );
  const waiting = await page.evaluate(async () => {
    const r = await navigator.serviceWorker.getRegistration();
    return { waiting: Boolean(r?.waiting), worker: navigator.serviceWorker.controller?.scriptURL, caches: await caches.keys(), page: document.documentElement.dataset.offline };
  });
  check(
    waiting.waiting && waiting.worker === was.worker && waiting.caches.includes("scaena-site-newer") && was.caches.every((c) => waiting.caches.includes(c)),
    `it waits beside the worker answering the page, which keeps its build: ${JSON.stringify(waiting)}`,
  );
  // Pressed, the new worker takes over, and the page loads again, answered by it alone.
  await Promise.all([page.waitForEvent("load", { timeout: 60000 }), page.click("#update")]);
  check(!(await opened(page, () => window.scaena?.last?.())), "reload: the editor opens again");
  await page.waitForFunction(() => document.documentElement.dataset.offline === "ready", null, { timeout: 60000 }).catch(() => {});
  const took = await page.evaluate(async () => {
    const r = await navigator.serviceWorker.getRegistration();
    return { waiting: Boolean(r?.waiting), controlled: Boolean(navigator.serviceWorker.controller), caches: (await caches.keys()).filter((c) => c.startsWith("scaena-site-")), offered: !document.querySelector("#update")?.hidden };
  });
  check(
    took.controlled && !took.waiting && !took.offered && JSON.stringify(took.caches) === JSON.stringify(["scaena-site-newer"]),
    `answered by the new build alone, the old build's cache gone: ${JSON.stringify(took)}`,
  );
  // The build's own sw.js again, which the pages opened offline cannot look for.
  newer = undefined;

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
  const [download] = await Promise.all([page.waitForEvent("download", { timeout: 60000 }), page.click("#file-open").then(() => page.click("#download"))]).catch(() => [undefined]);
  const zip = download ? await readFile(await download.path()) : Buffer.alloc(0);
  check(zip[0] === 0x50 && zip[1] === 0x4b, `Download works, the subsetter loaded from what the site keeps: ${zip.length} bytes`);

  // Offline, a deck in each theme, made and saved in the browser with its fonts alone (PLAN T.0c).
  for (const theme of shipped) {
    const title = `An offline ${theme.slug} deck`;
    await page.evaluate(([name, title]) => window.scaena.open({ create: { theme: name, title } }), [theme.name, title]);
    const name = `an-offline-${theme.slug}-deck`;
    await page.waitForFunction((name) => (window.scaena.last() && window.scaena.where().name === name) || document.querySelector("#status")?.textContent.startsWith("Error"), name, { timeout: 120000 }).catch(() => {});
    const made = await page.evaluate(() => ({ valid: window.scaena.last()?.valid, errors: window.scaena.last()?.findings.filter((f) => f.severity === "error").length }));
    const saved = await page.evaluate(() => window.scaena.save()).catch((e) => ({ error: String(e) }));
    const fonts = await page.evaluate(async (name) => {
      const dir = await (await (await navigator.storage.getDirectory()).getDirectoryHandle("bundles")).getDirectoryHandle(name);
      const out = [];
      for await (const [file] of (await dir.getDirectoryHandle("fonts")).entries()) out.push(file);
      return out.length;
    }, name).catch(() => -1);
    check(made.valid && made.errors === 0 && saved?.where?.kind === "opfs" && fonts === theme.fonts, `offline, ${theme.name} makes a deck and saves it in the browser with its ${theme.fonts} fonts: ${JSON.stringify({ ...made, saved: saved?.where ?? saved, fonts })}`);
  }

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

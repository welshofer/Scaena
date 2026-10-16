// PLAN O.1c map check: Insert › Map… in the editor, in headless Chromium (serve.mjs), Geoapify
// played by routes that answer with the recorded answers in tests/fixtures/maps. On the revenue
// example:
//
// - With no Geoapify key kept, the sheet asks for one and fetches nothing until it has one; the
//   key goes in the request's `apiKey` alone, and never into the deck.
// - On a slide started in Photo, a name that matches three places offers them; the second picked,
//   the map fills the empty place for a picture, its picture in the bundle named by its content,
//   its pin, its label, and its record on the image; one undo takes it back.
// - The map selected, the Format tab heads its Map section with where it came from; Zoom In fetches
//   it again a step nearer, one patch, which one undo takes back; Fetch Again asks for it as it was.
// - axe-core finds nothing against WCAG 2.1 AA in the sheet, nor in the Format tab with a map.
// - Downloaded, the bundle keeps the map's picture and no key; exported from it as one file, opened
//   offline, it draws the map, and how its slide reads names the map and says its credit.
// - Offline, the sheet says so and nothing changes.
//
//   node web/map.mjs     (after `just web`; from the repository's root)
//
// Exits 1 on any failure.
import { execFileSync } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { join } from "node:path";
import { launch, serve } from "./serve.mjs";

const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};

const fixture = (name) => readFile(`tests/fixtures/maps/${name}`);
const axe = createRequire(import.meta.url).resolve("axe-core/axe.min.js");

/** What axe-core finds against WCAG 2.1 A and AA on `page`. */
async function audit(page, what) {
  await page.addScriptTag({ path: axe });
  const found = await page.evaluate(async () => {
    const result = await window.axe.run(document, { runOnly: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"] });
    return result.violations.map((v) => `${v.id} (${v.impact}): ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
  });
  check(!found.length, `${what}: axe finds nothing against WCAG 2.1 AA${found.length ? `:\n    ${found.join("\n    ")}` : ""}`);
}
const asked = [];

const site = await serve();
const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1920, height: 1200 }, deviceScaleFactor: 1 });
  // Geoapify, played by its recorded answers: the geocoder by the place asked, the static map by
  // the recorded picture. Each request is kept as it was sent.
  const cors = { "access-control-allow-origin": "*" };
  await context.route("https://api.geoapify.com/**", async (route) => {
    const url = route.request().url();
    asked.push(url);
    const body = await fixture(url.includes("text=Springfield") ? "places-several.json" : "places-one.json");
    await route.fulfill({ status: 200, headers: { ...cors, "content-type": "application/json" }, body });
  });
  await context.route("https://maps.geoapify.com/**", async (route) => {
    asked.push(route.request().url());
    await route.fulfill({ status: 200, headers: { ...cors, "content-type": "image/png" }, body: await fixture("map.png") });
  });
  const page = await context.newPage();
  page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
  page.on("console", (m) => m.type() === "error" && !/Failed to load resource/.test(m.text()) && failures.push(`console: ${m.text()}`));
  await page.goto(`${site.origin}/web/dist/editor.html?painter=cpu&bundle=/docs/examples/revenue.deck.json`);
  await page.waitForFunction(() => window.scaena?.last()?.valid && window.scaena.canvas.boxes()?.length, null, { timeout: 120000 });

  const source = () => page.evaluate(() => window.scaena.source());
  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  const says = async (text) => {
    await page.waitForFunction((t) => document.querySelector("#status").textContent.includes(t), text, { timeout: 30000 }).catch(() => {});
    return (await status()).includes(text);
  };
  const back = async (to) => {
    await page.waitForFunction((s) => window.scaena.source() === s, to, { timeout: 30000, polling: 50 }).catch(() => {});
    return (await source()) === to;
  };
  const undo = async () => {
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+z");
  };
  const sheet = () =>
    page.evaluate(() => ({
      open: document.querySelector("#mapping").open,
      key: !document.querySelector("#mapping-key-row").hidden,
      goes: document.querySelector("#mapping-goes").textContent,
      status: document.querySelector("#mapping-status").textContent,
      places: [...document.querySelectorAll("#mapping-places button")].map((b) => b.textContent),
    }));
  const fromInsert = async () => {
    await page.selectOption("#insert", "insert-map");
    await page.waitForFunction(() => document.querySelector("#mapping").open, null, { timeout: 10000 }).catch(() => {});
    return sheet();
  };

  // A slide started in Photo, whose empty place for a picture the map fills.
  await page.locator('#strip li[data-state="mix"]').click();
  await page.waitForFunction(() => window.scaena.at()?.index === 2, null, { timeout: 30000 }).catch(() => {});
  await page.click("#strip [data-add=slide]");
  await page.waitForSelector("#starting[open] button[data-start]", { timeout: 60000 });
  await page.locator("#starting").getByRole("tab", { name: "Pictures", exact: true }).click();
  await page.locator("#starting").getByRole("button", { name: "Photo", exact: true }).click();
  await page.waitForFunction(() => window.scaena.last()?.states[window.scaena.shown()]?.[0] === "photo" && window.scaena.canvas.boxes(), null, { timeout: 30000 }).catch(() => {});
  const started = await source();
  check(started.includes("state photo"), "Photo starts a slide");

  // 1. No key kept: the sheet asks for one, and nothing is fetched without it.
  const keyless = await fromInsert();
  check(keyless.open && keyless.key, `with no key, the sheet asks for one: ${JSON.stringify(keyless)}`);
  check(keyless.goes === "It fills the empty place for a picture on this slide.", `and says where it goes: ${keyless.goes}`);
  await page.fill("#mapping-where", "Springfield");
  await page.click("#mapping-fetch");
  await page.evaluate(() => window.scaena.mapping.settled());
  check(asked.length === 0 && /needs a key of your own/.test((await sheet()).status), `nothing fetched without a key: ${(await sheet()).status}`);

  // 2. A key, and a name that matches three places: offered, and the second picked.
  await page.fill("#mapping-key", "map-test-key");
  await page.click("#mapping-fetch");
  await page.evaluate(() => window.scaena.mapping.settled());
  const offered = await sheet();
  check(offered.places.length === 3 && offered.places[1] === "Springfield, MA, United States of America", `three places offered: ${JSON.stringify(offered.places)}`);
  check(asked.length === 1 && asked[0].includes("text=Springfield") && asked[0].endsWith("&apiKey=map-test-key"), `the geocoder asked once, the key in its query: ${asked[0]}`);
  await audit(page, "the sheet, offering places");
  await page.locator("#mapping-places button").nth(1).click();
  await page.evaluate(() => window.scaena.mapping.settled());
  check(await says("Map of Springfield from Geoapify added"), `the status says so: ${await status()}`);
  const mapped = await source();
  check(/map image .*"assets\/[0-9a-f]{64}\.png"/s.test(mapped) || /\bmap image\b/.test(mapped), "an image of the map joins the deck");
  check(mapped.includes('from: geoapify') || mapped.includes('from:geoapify') || mapped.includes('"geoapify"'), "its record says where it came from");
  check(!mapped.includes("map-test-key"), "no key in the deck");
  const statics = asked.filter((u) => u.startsWith("https://maps.geoapify.com/"));
  check(statics.length === 1 && statics[0].includes("lonlat:-72.5898,42.1015") && statics[0].includes("style=dark-matter"), `the static map of the place picked, in the style Dusk's surface asks: ${statics[0]}`);
  check(await page.evaluate(() => window.scaena.canvas.selected() === "map"), "the map is selected");
  await undo();
  check(await back(started), "one undo takes it back");
  await page.keyboard.press("Control+Shift+z");
  check(await back(mapped), "and redo puts it there again");

  // 3. Zoom In from the Format tab's Map section: fetched again a step nearer, one patch.
  await page.evaluate(() => window.scaena.canvas.select("map"));
  await page.waitForSelector("#look .map-title", { timeout: 30000 });
  const heading = await page.evaluate(() => document.querySelector("#look .map-title + .named")?.textContent);
  check(/^Map from Geoapify, fetched \d+ \w+ \d{4}$/.test(heading ?? ""), `the Map section says where it came from: ${heading}`);
  await audit(page, "the Format tab with a map selected");
  const before = asked.length;
  await page.click('#look [data-remap="in"]');
  check(await says("fetched a step nearer"), `Zoom In says so: ${await status()}`);
  const nearer = await source();
  const again = asked.slice(before);
  check(again.length === 1 && again[0].includes("&zoom=") && again[0].startsWith("https://maps.geoapify.com/"), `one static map asked, no geocoder: ${again}`);
  await undo();
  check(await back(mapped), "one undo takes the nearer map back");
  await page.keyboard.press("Control+Shift+z");
  check(await back(nearer), "and redo puts it back");

  // 4. Downloaded: the picture kept, no key. Exported as one file and opened offline: it draws.
  const [download] = await Promise.all([page.waitForEvent("download"), page.click("#download")]);
  const zip = await readFile(await download.path());
  const asset = nearer.match(/"(assets\/[0-9a-f]{64}\.png)"/)?.[1];
  check(!!asset && zip.includes(Buffer.from(asset.replace(/^assets\//, ""))) && !zip.includes(Buffer.from("map-test-key")), `the download keeps the map's picture and no key: ${asset}`);
  const out = join(process.cwd(), "target/map");
  await mkdir(out, { recursive: true });
  await writeFile(join(out, "map.scaena"), zip);
  const single = join(out, "map.html");
  execFileSync("cargo", ["run", "-q", "-p", "scaena-cli", "--locked", "--", "export", join(out, "map.scaena"), "--format", "html", "--out", single], { stdio: "inherit" });
  const html = await readFile(single, "utf8");
  check(/Map of Springfield/.test(html) && /OpenStreetMap contributors/.test(html), "the single file reads the map as the map of its place, then its credit");
  const offline = await browser.newContext({ viewport: { width: 1280, height: 720 }, deviceScaleFactor: 1, offline: true });
  const player = await offline.newPage();
  player.on("pageerror", (e) => failures.push(`single file: ${e.message}`));
  await player.goto(`file://${single}?painter=cpu`);
  await player.waitForFunction(() => window.scaena?.at?.(), null, { timeout: 120000 });
  const states = await player.evaluate(() => window.scaena.states);
  const photo = states.indexOf("photo");
  await player.evaluate((n) => window.scaena.seek(n), photo);
  await player.waitForFunction((n) => window.scaena.at()?.index === n, photo, { timeout: 30000 }).catch(() => {});
  await player.waitForTimeout(1500);
  const shot = await player.locator("canvas").first().screenshot({ path: join(out, "photo.png") });
  check(shot.length > 20000, `offline, the single file draws the map's slide: ${shot.length} bytes of PNG`);
  await offline.close();

  // 5. Offline: said, and nothing changes.
  await context.setOffline(true);
  const now = await source();
  const sent = asked.length;
  await fromInsert();
  await page.fill("#mapping-where", "Ferry Building");
  await page.click("#mapping-fetch");
  await page.evaluate(() => window.scaena.mapping.settled());
  check(/offline/.test((await sheet()).status) && asked.length === sent && (await source()) === now, `offline, it says so and changes nothing: ${(await sheet()).status}`);
  await context.setOffline(false);
} finally {
  await browser.close();
  site.close();
}

if (failures.length) {
  console.error(`\n${failures.length} failure(s)`);
  process.exit(1);
}
console.log("\nInsert › Map… checks out");

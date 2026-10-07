// PLAN 2.75 check: the canvas by keys alone (WCAG 2.1.1), in the editor in headless Chromium
// (serve.mjs), the CPU painting: on the torture deck's `containers`, `shapes`, and `images` cases,
// and on the revenue example's chart and layout.
//
//   node web/bykeys.mjs     (after `just web`; from the repository's root)
//
// - Tab and Shift+Tab select the state's nodes in reading order; past the last, nothing is selected
//   and the focus leaves the canvas. Enter goes into a stack, and Escape back out.
// - Enter on a shape goes to its points, on an image to its crop's sides and focal point, on a
//   chart to its marks: Tab steps through them, an arrow moves a handle, each move the patch its drag
//   makes, one step to undo; + adds a point, and a mark reached is picked, so its menu annotates it.
// - [ and ] turn what is selected; ⌘A selects all beside it.
// - In a layout shown, Tab keys a slot and an arrow moves it a track: one theme edit.
// - The keys sheet lists the keys, and axe-core finds nothing with a handle keyed.
// Exits 1 on any failure.
import { createRequire } from "node:module";
import { launch, serve } from "./serve.mjs";

const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};

const axe = createRequire(import.meta.url).resolve("axe-core/axe.min.js");
const site = await serve();
const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1 });
  /** The editor on `bundle`, showing `state`. */
  const open = async (bundle, state) => {
    const page = await context.newPage();
    page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
    page.on("console", (m) => m.type() === "error" && failures.push(`console: ${m.text()}`));
    await page.goto(`${site.origin}/web/dist/editor.html?painter=cpu&bundle=${bundle}`);
    await page.waitForFunction(() => window.scaena?.last()?.valid, null, { timeout: 120000 });
    await show(page, state);
    return page;
  };
  const show = async (page, state) => {
    await page.selectOption("#state", state);
    await page.waitForFunction((s) => window.scaena.canvas.boxed() === s, state, { timeout: 30000 }).catch(() => {});
  };
  const selected = (page) => page.evaluate(() => window.scaena.canvas.selected());
  const status = (page) => page.evaluate(() => document.querySelector("#status").textContent);
  const press = async (page, key) => {
    await page.keyboard.press(key);
    await page.waitForTimeout(50);
  };
  const comes = (page, test) =>
    page
      .waitForFunction((t) => new RegExp(t).test(window.scaena.source()), test, { timeout: 30000, polling: 50 })
      .then(() => true)
      .catch(() => false);
  const back = (page, to) =>
    page
      .waitForFunction((s) => window.scaena.source() === s, to, { timeout: 30000, polling: 50 })
      .then(() => true)
      .catch(() => false);
  const undo = async (page) => {
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+z");
  };
  /** Once the handle keyed is of `kind`. */
  const keyed = (page, kind) =>
    page
      .waitForFunction((k) => window.scaena.canvas.handle()?.kind === k, kind, { timeout: 30000 })
      .then(() => page.evaluate(() => window.scaena.canvas.handle()))
      .catch(() => page.evaluate(() => window.scaena.canvas.handle()));

  // The torture deck's containers: Tab through the canvas's nodes.
  const page = await open("/tests/fixtures/torture.scaena/deck.json", "containers");
  const original = await page.evaluate(() => window.scaena.source());
  await page.locator("#overlay").focus();
  await press(page, "Tab");
  const first = await selected(page);
  check(first !== undefined && (await status(page)).includes("1 of"), `Tab selects the first node in reading order: ${first} · ${await status(page)}`);
  const seen = [first];
  for (let i = 0; i < 12; i++) {
    await press(page, "Tab");
    const now = await selected(page);
    if (now === undefined) break;
    seen.push(now);
  }
  check(seen.includes("stats") && new Set(seen).size === seen.length, `each Tab the next, none twice: ${seen.join(", ")}`);
  check((await selected(page)) === undefined && (await page.evaluate(() => document.activeElement?.id)) !== "overlay", "past the last, nothing is selected and the focus leaves the canvas");
  await page.locator("#overlay").focus();
  await press(page, "Shift+Tab");
  check((await selected(page)) === seen.at(-1), `Shift+Tab from outside selects the last: ${await selected(page)}`);
  await press(page, "Shift+Tab");
  check((await selected(page)) === seen.at(-2), "Shift+Tab, the one before");
  // Into the stack, and out.
  await page.evaluate(() => window.scaena.canvas.select("stats"));
  await page.locator("#overlay").focus();
  await press(page, "Enter");
  check((await selected(page)) === "stat-a", `Enter goes into the stack: ${await selected(page)}`);
  await press(page, "Tab");
  check((await selected(page)) === "stat-b", `Tab goes on among what it holds: ${await selected(page)}`);
  await press(page, "Escape");
  check((await selected(page)) === "stats", "Escape goes back out");

  // A polygon's points: Enter, an arrow, +.
  await show(page, "shapes");
  await page.evaluate(() => window.scaena.canvas.select("shape-tri"));
  await page.waitForFunction(() => window.scaena.canvas.outlined()?.node === "shape-tri", null, { timeout: 30000 }).catch(() => {});
  await page.locator("#overlay").focus();
  await press(page, "Enter");
  const point = await keyed(page, "point");
  check(point?.index === 0, `Enter on a shape keys its first point: ${JSON.stringify(point)}`);
  await press(page, "Tab");
  await press(page, "ArrowLeft");
  check(await comes(page, "points:\\[\\[0\\.5, 0\\], \\[0\\.99, 1\\], \\[0, 1\\]\\]"), `← moves the second point a hundredth left: ${(await page.evaluate(() => window.scaena.source())).match(/shape-tri[^\n]*/)?.[0]}`);
  await undo(page);
  check(await back(page, original), "one undo takes it back");
  // The undo compiled and laid out again: the outline is the source's.
  await page.waitForFunction(() => JSON.stringify(window.scaena.canvas.outlined()?.points) === "[[0.5,0],[1,1],[0,1]]", null, { timeout: 30000 }).catch(() => {});
  await page.locator("#overlay").focus();
  await press(page, "Escape");
  await press(page, "Enter");
  await keyed(page, "point");
  await press(page, "+");
  check(await comes(page, "points:\\[\\[0\\.5, 0\\], \\[0\\.75, 0\\.5\\], \\[1, 1\\]"), "+ adds a point after the one keyed, at the edge's middle");
  await undo(page);
  await back(page, original);

  // An image's crop and focal point.
  await show(page, "images");
  await page.evaluate(() => window.scaena.canvas.select("image-cover"));
  await page.waitForFunction(() => window.scaena.canvas.framing()?.node === "image-cover", null, { timeout: 30000 }).catch(() => {});
  await page.locator("#overlay").focus();
  await press(page, "Enter");
  check((await keyed(page, "crop"))?.side === "n", "Enter on an image keys its crop's top");
  await press(page, "Tab");
  check((await keyed(page, "crop"))?.side === "e", "Tab, its right");
  await page.addScriptTag({ path: axe });
  const found = await page.evaluate(async () => {
    const result = await window.axe.run(document, { runOnly: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"] });
    return result.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
  });
  check(!found.length, `axe finds nothing with a handle keyed${found.length ? `:\n    ${found.join("\n    ")}` : ""}`);
  check((await page.locator("#overlay svg .keyed").count()) === 1, "the handle keyed is marked");
  await press(page, "ArrowLeft");
  check(await comes(page, "crop:\\[0, 0, 0\\.99, 1\\]"), "← crops a hundredth from the right");
  await undo(page);
  check(await back(page, original), "one undo takes it back");
  await page.evaluate(() => window.scaena.canvas.select("image-cover"));
  await page.waitForFunction(() => window.scaena.canvas.framing()?.node === "image-cover", null, { timeout: 30000 }).catch(() => {});
  await page.locator("#overlay").focus();
  await press(page, "Escape");
  await press(page, "Enter");
  for (let i = 0; i < 4; i++) await press(page, "Tab");
  check((await keyed(page, "focal"))?.kind === "focal", "four Tabs on, its focal point");
  await press(page, "Shift+ArrowRight");
  check(await comes(page, "focal:\\[0\\.6, 0\\.5\\]"), "Shift+→ moves it a tenth");
  await press(page, "Escape");
  check((await page.evaluate(() => window.scaena.canvas.handle())) === undefined && (await selected(page)) === "image-cover", "Escape leaves the handles, the image selected");
  await undo(page);
  await back(page, original);

  // [ and ] turn.
  await page.evaluate(() => window.scaena.canvas.select("image-cover"));
  await page.locator("#overlay").focus();
  await press(page, "]");
  check(await comes(page, "image-cover[\\s\\S]*rotate: 15"), "] turns what is selected 15°");
  await undo(page);
  check(await back(page, original), "one undo takes it back");
  await page.close();

  // The revenue example: a chart's marks, ⌘A, and a layout's slots.
  const rev = await open("/docs/examples/revenue.deck.json", "revenue");
  await rev.evaluate(() => window.scaena.canvas.select("rev"));
  await rev.waitForFunction(() => window.scaena.canvas.selected() === "rev", null, { timeout: 30000 });
  await rev.waitForTimeout(500);
  await rev.locator("#overlay").focus();
  await press(rev, "Enter");
  const mark = await keyed(rev, "mark");
  check(mark?.kind === "mark" && (await rev.evaluate(() => window.scaena.canvas.picked()?.key)) === mark.mark.key, `Enter on a chart picks its first mark: ${mark?.label}`);
  await press(rev, "Tab");
  const next = await rev.evaluate(() => window.scaena.canvas.picked()?.key);
  check(next !== undefined && next !== mark?.mark.key, `Tab picks the next: ${next}`);
  await press(rev, "Shift+F10");
  const menu = await rev
    .waitForSelector(".context-menu", { timeout: 10000 })
    .then(() => rev.evaluate(() => [...document.querySelectorAll(".context-menu [role=menuitem] span")].map((b) => b.textContent)))
    .catch(() => []);
  check(menu.some((l) => /highlight/i.test(l)), `its menu annotates it: ${menu.slice(0, 4).join(", ")}`);
  await press(rev, "Escape");
  await rev.locator("#overlay").focus();
  await press(rev, "Escape");
  await press(rev, "Escape");
  await press(rev, "Control+a");
  const all = await rev.evaluate(() => window.scaena.canvas.chosen());
  check(all.length > 2, `⌘A selects all on the canvas: ${all.join(", ")}`);
  // The layout: Tab keys a slot, → moves it a track.
  await press(rev, "Escape");
  await rev.click("#layout-edit");
  await rev.waitForFunction(() => window.scaena.canvas.slotting()?.layout === "figure", null, { timeout: 30000 }).catch(() => {});
  await rev.locator("#overlay").focus();
  await press(rev, "Tab");
  check((await status(rev)).includes("slot kicker, 1 of 4"), `Tab keys the first slot: ${await status(rev)}`);
  const kicker = await rev.evaluate(() => window.scaena.canvas.slotting().slots.find((s) => s.name === "kicker").col);
  await press(rev, "ArrowLeft");
  const moved = await rev
    .waitForFunction((was) => JSON.stringify(window.scaena.canvas.slotting()?.slots.find((s) => s.name === "kicker")?.col) !== JSON.stringify(was), kicker, { timeout: 30000 })
    .then(() => rev.evaluate(() => window.scaena.canvas.slotting().slots.find((s) => s.name === "kicker").col))
    .catch(() => null);
  check(moved && moved[0] === kicker[0] - 1, `← moves it a track: ${JSON.stringify(kicker)} → ${JSON.stringify(moved)} · ${await status(rev)}`);
  await press(rev, "Control+z");
  await press(rev, "Escape");
  await press(rev, "Escape");

  // The keys sheet.
  await press(rev, "?");
  await rev.waitForSelector("#keys[open]", { timeout: 10000 }).catch(() => {});
  const sheet = await rev.evaluate(() => [...document.querySelectorAll("#keys tr")].map((tr) => tr.textContent));
  const lists = (s) => sheet.some((l) => l.includes(s));
  check(lists("Tab, Shift+Tab") && lists("Enter on a shape or an image") && lists("[ ]") && lists("Key a slot"), "the keys sheet lists Tab, Enter, the handles' keys, [ ], and the slots'");
} finally {
  await browser.close();
  await site.close();
}

if (failures.length) {
  console.log(`\n${failures.length} failed:\n  ${failures.join("\n  ")}`);
  process.exit(1);
}
console.log("\nall passed");

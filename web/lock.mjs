// PLAN 2.95 check: a node locked, in the editor, in headless Chromium (serve.mjs), the CPU
// painting, on the trails example's storm slide, whose photo fills the canvas under its title.
//
//   node web/lock.mjs     (after `just web`; from the repository's root)
//
// - The lock in the Layers tab locks the photo: one patch of its own `locked`, one step to undo.
// - The canvas passes over it: a click on it selects nothing, and the marquee and ⌘A take the
//   title and the caption over it, not it.
// - Selected in the layers, it is outlined, with no handles. Its arrow keys, `]`, and Delete do
//   nothing to it, and the status says why and how to unlock it.
// - A right click on it offers to unlock it; ⇧⌘L locks what is selected, and unlocks it.
// - axe-core finds nothing against WCAG 2.1 AA in the Layers tab.
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
  const page = await context.newPage();
  page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
  page.on("console", (m) => m.type() === "error" && failures.push(`console: ${m.text()}`));
  await page.goto(`${site.origin}/web/dist/editor.html?painter=cpu&bundle=/docs/examples/trails.deck.json`);
  await page.waitForFunction(() => window.scaena?.last()?.valid, null, { timeout: 120000 });

  const source = () => page.evaluate(() => window.scaena.source());
  const status = () => page.locator("#status").textContent();
  const selected = () => page.evaluate(() => window.scaena.canvas.selected());
  const chosen = () => page.evaluate(() => window.scaena.canvas.chosen?.() ?? [window.scaena.canvas.selected()].filter(Boolean));
  const says = (words) => page.waitForFunction((w) => document.querySelector("#status").textContent.includes(w), words, { timeout: 30000 }).then(() => true, () => false);
  const photoLine = async () => (await source()).split("\n").find((l) => l.includes("semantic:evidence") && l.includes("z:-10")) ?? "";
  const lockedNow = async () => (await photoLine()).includes("locked:true");
  const settled = async (was) => {
    await page.waitForFunction((s) => window.scaena.source() !== s, was, { timeout: 30000 }).catch(() => {});
    await page.waitForFunction(() => window.scaena.canvas.boxes().length > 0, null, { timeout: 30000 });
  };
  /** A client point on the canvas at canvas point `[x, y]`. */
  const client = (x, y) =>
    page.evaluate(([px, py]) => {
      const r = document.querySelector("#overlay").getBoundingClientRect();
      const [vx, vy, vw, vh] = window.scaena.canvas.view();
      return [r.left + ((px - vx) / vw) * r.width, r.top + ((py - vy) / vh) * r.height];
    }, [x, y]);

  // The storm slide.
  await page.selectOption("#state", "storm");
  await page.waitForFunction(() => window.scaena.canvas.boxed() === "storm" && window.scaena.canvas.boxes().length > 0, null, { timeout: 30000 });
  const original = await source();
  // A point on the photo clear of the slide's texts: four fifths across, halfway down.
  const [px, py] = await client(1920 * 0.8, 1080 * 0.5);
  await page.mouse.click(px, py);
  await page.waitForTimeout(300);
  check((await selected()) === "storm-photo", `unlocked, a click on the photo selects it: ${await selected()}`);

  // The lock in the layers.
  await page.click("#tab-layers");
  const lockButton = page.locator('#layers [data-layer="storm-photo"] [data-lock]').first();
  await lockButton.waitFor({ timeout: 30000 });
  check((await lockButton.getAttribute("aria-pressed")) === "false", "its lock is open");
  await lockButton.click();
  await settled(original);
  check(await lockedNow(), `the lock writes the photo's own locked: ${await photoLine()}`);
  check(await says("storm-photo locked: the canvas passes over it"), `the status says so: ${await status()}`);
  await page.waitForFunction(() => document.querySelector('#layers [data-layer="storm-photo"] [data-lock]')?.getAttribute("aria-pressed") === "true", null, { timeout: 30000 }).catch(() => {});
  check((await lockButton.getAttribute("aria-pressed")) === "true", "its lock is closed");
  const lockedSource = await source();

  // The canvas passes over it.
  await page.evaluate(() => window.scaena.canvas.select(undefined));
  await page.mouse.click(px, py);
  await page.waitForTimeout(300);
  check((await selected()) === undefined, `a click on the locked photo selects nothing: ${await selected()}`);
  const [ax, ay] = await client(4, 4);
  const [bx, by] = await client(1916, 1076);
  await page.mouse.move(ax, ay);
  await page.mouse.down();
  await page.mouse.move(bx, by, { steps: 8 });
  await page.mouse.up();
  await page.waitForTimeout(300);
  const marqueed = (await chosen()).sort();
  check(marqueed.join() === "storm-note,storm-title", `the marquee takes the title and the caption, not the photo: ${marqueed.join(", ")}`);
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+a");
  await page.waitForTimeout(200);
  const all = (await chosen()).sort();
  check(!all.includes("storm-photo") && all.length === 2, `⌘A passes over it: ${all.join(", ")}`);

  // Selected in the layers: outlined, with no handles, and no key moves or deletes it.
  await page.click('#layers [data-layer="storm-photo"] [data-pick]');
  await page.waitForFunction(() => window.scaena.canvas.selected() === "storm-photo", null, { timeout: 30000 }).catch(() => {});
  check((await selected()) === "storm-photo", "the layers select it");
  const drawn = await page.evaluate(() => ({
    locked: document.querySelectorAll("#overlay .selected.locked").length,
    handles: document.querySelectorAll("#overlay .handle").length,
  }));
  check(drawn.locked === 1 && drawn.handles === 0, `it is outlined, with no handles: ${JSON.stringify(drawn)}`);
  await page.locator("#overlay").focus();
  for (const [key, words] of [
    ["ArrowRight", "nothing moves"],
    ["]", "nothing turns"],
    ["Delete", "nothing is deleted"],
  ]) {
    await page.keyboard.press(key);
    check(await says(words), `${key} does nothing to it, and says why: ${await status()}`);
    check(await says("⌘⇧L unlocks it") || (await status()).includes("unlocks it"), `and how to unlock it: ${await status()}`);
  }
  check((await source()) === lockedSource, "the source is as the lock left it");

  // A right click on it offers to unlock it.
  await page.evaluate(() => window.scaena.canvas.select(undefined));
  await page.mouse.click(px, py, { button: "right" });
  await page.waitForSelector(".context-menu", { timeout: 10000 }).catch(() => {});
  const offered = await page.evaluate(() => [...document.querySelectorAll(".context-menu [role=menuitem] span")].map((s) => s.textContent));
  check(offered.includes("Unlock storm-photo"), `the menu offers to unlock it: ${offered.join(", ")}`);
  await page.locator(".context-menu [role=menuitem]", { hasText: "Unlock storm-photo" }).first().click();
  await settled(lockedSource);
  check(!(await lockedNow()), `unlocked: ${await photoLine()}`);
  check(await says("storm-photo unlocked"), `the status says so: ${await status()}`);

  // ⌘Z locks it again, one step, and again leaves it as it was.
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+z");
  await page.waitForFunction((s) => window.scaena.source() === s, lockedSource, { timeout: 30000 }).catch(() => {});
  check((await source()) === lockedSource, "⌘Z locks it again");
  await page.keyboard.press("Control+z");
  await page.waitForFunction((s) => window.scaena.source() === s, original, { timeout: 30000 }).catch(() => {});
  check((await source()) === original, "and again, the deck is as it was");

  // ⇧⌘L locks what is selected, and unlocks it.
  await page.evaluate(() => window.scaena.canvas.select("storm-title"));
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+Shift+L");
  await settled(original);
  const titled = (await source()).split("\n").find((l) => l.includes("storm-title text")) ?? "";
  const titleLocked = await page.evaluate(() => window.scaena.canvas.boxes().find((b) => b.node === "storm-title")?.locked);
  check(titleLocked === "storm-title", `⇧⌘L locks the title: ${titleLocked} · ${titled}`);
  const before = await source();
  await page.keyboard.press("Control+Shift+L");
  await settled(before);
  check((await source()) === original, "⇧⌘L again unlocks it, the deck as it was");

  // Nothing against WCAG 2.1 AA in the layers.
  await page.addScriptTag({ path: axe });
  const found = await page.evaluate(async () => {
    const result = await window.axe.run({ include: [["#layers"]] }, { runOnly: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"] });
    return result.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
  });
  check(found.length === 0, `axe finds nothing in the layers: ${found.join("; ")}`);
} finally {
  await browser.close();
  await site.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "a node locked is passed over on the canvas, kept from its keys, and unlocked");
process.exit(failures.length ? 1 : 0);

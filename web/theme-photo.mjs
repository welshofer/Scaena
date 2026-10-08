// PLAN 2.94 check: a theme's colors from a photo the bundle holds, in the editor, in headless
// Chromium (serve.mjs), the CPU painting, on the trails example.
//
//   node web/theme-photo.mjs     (after `just web`; from the repository's root)
//
// - The Theme tab lists each photo the bundle holds, with the nodes drawn from it.
// - A click gives the theme the photo's colors, as `scaena theme --from-photo` does. The ridge
//   photo's violet becomes the accent. The deck is drawn in the new colors, and its source is as it
//   was. The status says what changed, and ⌘Z in the source writes the theme back.
// - The same photo again changes nothing, and says so.
// - A right click on the photo on the canvas offers the same, from the photo that node draws.
// - axe-core finds nothing against WCAG 2.1 AA with the tab shown.
// Exits 1 on any failure.
import { createRequire } from "node:module";
import { launch, serve } from "./serve.mjs";

const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};
const RIDGE = "assets/trails-ridge.png";

const axe = createRequire(import.meta.url).resolve("axe-core/axe.min.js");
const site = await serve();
const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1 });
  const page = await context.newPage();
  page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
  page.on("console", (m) => m.type() === "error" && failures.push(`console: ${m.text()}`));
  await page.goto(`${site.origin}/web/dist/editor.html?painter=cpu&source=open&bundle=/docs/examples/trails.deck.json`);
  await page.waitForFunction(() => window.scaena?.last()?.valid, null, { timeout: 120000 });

  const source = () => page.evaluate(() => window.scaena.source());
  const status = () => page.locator("#status").textContent();
  const settled = () => page.evaluate(() => window.scaena.theme.settled());
  const accent = () => page.evaluate(() => window.scaena.theme.theme()?.tokens?.color?.accent);
  const shot = async () => {
    await page.waitForTimeout(400);
    return page.locator("#stage").screenshot();
  };
  const accentIs = (value) =>
    page.waitForFunction((v) => window.scaena.theme.theme()?.tokens?.color?.accent === v, value, { timeout: 30000 }).catch(() => {});

  // The storm slide, its photo full bleed.
  await page.selectOption("#state", "storm");
  await page.waitForFunction(() => window.scaena.canvas.boxed() === "storm", null, { timeout: 30000 }).catch(() => {});
  await page.click("#tab-theming");
  await page.waitForFunction(() => window.scaena.theme.theme() !== undefined, null, { timeout: 60000 });
  await page.waitForSelector(`#theming [data-photo="${RIDGE}"]`, { timeout: 30000 }).catch(() => {});
  const listed = await page.evaluate(() =>
    [...document.querySelectorAll("#theming [data-photos] li")].map((li) => [...li.children].map((c) => c.textContent).join(" ")),
  );
  check(listed.join(" | ") === `${RIDGE} storm-photo`, `the tab lists the bundle's photo and what draws it: ${listed.join(" | ")}`);
  const original = await source();
  const was = await accent();
  check(was === "#FF6A3D", `Dusk's accent first: ${was}`);
  const before = await shot();

  // A click gives the theme the photo's colors.
  await page.click(`#theming [data-photo="${RIDGE}"]`);
  await settled();
  await accentIs("#B97CFF");
  check((await accent()) === "#B97CFF", `the ridge's violet is the accent: ${await accent()}`);
  check((await status()).includes(`theme: colors from ${RIDGE}`) && (await status()).includes("⌘Z undoes it"), `the status says so: ${await status()}`);
  check((await source()) === original, "the source is as it was: the theme is the bundle's file");
  check(!(await shot()).equals(before), "the deck is drawn in the photo's colors");

  // The same photo again changes nothing.
  await page.click(`#theming [data-photo="${RIDGE}"]`);
  await settled();
  await page.waitForFunction(() => document.querySelector("#status").textContent.includes("nothing to edit"), null, { timeout: 30000 }).catch(() => {});
  check((await status()).includes("nothing to edit"), `taken again, it says there is nothing to edit: ${await status()}`);

  // ⌘Z in the source writes the theme back.
  await page.locator(".cm-content").focus();
  await page.keyboard.press("Control+z");
  await accentIs(was);
  check((await accent()) === was, `⌘Z writes the theme back: ${await accent()}`);
  check((await shot()).equals(before), "the deck is drawn as it was");

  // A right click on the photo on the canvas offers the same.
  // A point on the photo clear of the slide's texts: four fifths across, halfway down.
  const box = await page.evaluate(() => {
    const b = window.scaena.canvas.boxes().find((x) => x.node === "storm-photo");
    if (!b) return undefined;
    const r = document.querySelector("#overlay").getBoundingClientRect();
    const [x, y, w, h] = window.scaena.canvas.view();
    const [px, py] = [b.rect[0] + b.rect[2] * 0.8, b.rect[1] + b.rect[3] * 0.5];
    return { x: r.left + ((px - x) / w) * r.width, y: r.top + ((py - y) / h) * r.height };
  });
  check(box !== undefined, "the storm photo has its box");
  if (box) {
    await page.mouse.click(box.x, box.y, { button: "right" });
    await page.waitForSelector(".context-menu", { timeout: 10000 }).catch(() => {});
    const offered = await page.evaluate(() => [...document.querySelectorAll(".context-menu [role=menuitem] span")].map((s) => s.textContent));
    check(offered.includes("Theme colors from this photo"), `the menu offers the photo's colors: ${offered.join(", ")}`);
    await page.locator(".context-menu [role=menuitem]", { hasText: "Theme colors from this photo" }).first().click();
    await settled();
    await accentIs("#B97CFF");
    check((await accent()) === "#B97CFF", `the menu gives the theme the photo's colors: ${await accent()}`);
  }

  // Nothing against WCAG 2.1 AA with the tab shown.
  await page.addScriptTag({ path: axe });
  const found = await page.evaluate(async () => {
    const result = await window.axe.run({ include: [["#theming"]] }, { runOnly: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"] });
    return result.violations.map((v) => `${v.id}: ${v.nodes.length}`);
  });
  check(found.length === 0, `axe finds nothing in the tab: ${found.join("; ")}`);
} finally {
  await browser.close();
  await site.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "a photo gives the theme its colors, and ⌘Z takes them back");
process.exit(failures.length ? 1 : 0);

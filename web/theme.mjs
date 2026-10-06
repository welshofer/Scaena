// PLAN 2.39 check: a theme chosen in the editor, in headless Chromium (serve.mjs), the CPU
// painting.
//
//   node web/theme.mjs     (after `just web`; from the repository's root)
//
// - On the revenue example, the theme picker offers the bundle's Dusk, chosen, and the themes that
//   ship. Daybreak chosen re-themes the deck: its source names `themes/daybreak.theme.json`, the
//   preview is drawn in it, the status says what lint finds new, and the picker offers it from the
//   bundle now. One undo puts Dusk back, drawn as before.
// - On the torture deck, Dusk is refused: the deck uses roles Dusk lacks. The status says why, and
//   the source and the picker are as they were.
// Exits 1 on any failure.
import { createHash } from "node:crypto";
import { launch, serve } from "./serve.mjs";

const server = await serve();
const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};

const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1920, height: 1200 }, deviceScaleFactor: 1 });
  const open = async (bundle) => {
    const page = await context.newPage();
    page.on("pageerror", (e) => failures.push(`${bundle}: ${e.message}`));
    page.on("console", (m) => m.type() === "error" && failures.push(`${bundle}: console: ${m.text()}`));
    await page.goto(`${server.origin}/web/dist/editor.html?painter=cpu&bundle=${bundle}`);
    await page.waitForFunction(() => window.scaena?.last(), null, { timeout: 120000 });
    await page.waitForFunction(() => document.querySelector("#theme")?.options.length > 0, null, { timeout: 30000 });
    return page;
  };
  const source = (page) => page.evaluate(() => window.scaena.source());
  const status = (page) => page.evaluate(() => document.querySelector("#status").textContent);
  const offered = (page) =>
    page.evaluate(() => [...document.querySelectorAll("#theme option")].map((o) => `${o.parentElement.label}/${o.textContent}${o.selected ? "*" : ""}`));
  /** The preview as painted, with nothing focused over it. */
  const drawn = async (page) => {
    await page.evaluate(() => document.activeElement?.blur());
    await page.mouse.move(0, 0);
    await page.waitForTimeout(100);
    return createHash("sha256").update(await page.locator("#stage").screenshot()).digest("hex");
  };
  /** Once the preview has painted what the source says. */
  const settled = (page) =>
    page.waitForFunction(() => window.scaena.last() && !window.scaena.canvas.busy(), null, { timeout: 30000 }).then(() => page.waitForTimeout(600));

  // The revenue example, in Dusk.
  const page = await open("/docs/examples/revenue.deck.json");
  const original = await source(page);
  await settled(page);
  const dusk = await drawn(page);
  const first = await offered(page);
  check(
    first.includes("In the bundle/dusk*") && ["Dusk", "Daybreak", "Ember"].every((t) => first.includes(`Ships/${t}`)),
    `the picker offers the bundle's Dusk, chosen, and the themes that ship: ${first.join(", ")}`,
  );

  // Daybreak, which ships.
  await page.selectOption("#theme", "ships:Daybreak");
  const named = await page
    .waitForFunction(() => window.scaena.source().includes('theme:"themes/daybreak.theme.json"'), null, { timeout: 60000 })
    .then(() => true, () => false);
  check(named, "Daybreak chosen, the source names it");
  check(/theme Daybreak · lint finds \d+ new, \d+ gone/.test(await status(page)), `the status says what lint finds: ${await status(page)}`);
  await settled(page);
  const day = await drawn(page);
  check(day !== dusk, "the preview is drawn in Daybreak");
  const after = await offered(page);
  check(after.includes("In the bundle/daybreak*") && after.includes("In the bundle/dusk"), `the bundle holds it now, chosen: ${after.join(", ")}`);

  // One undo: Dusk again, drawn as it was.
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+z");
  const back = await page.waitForFunction((s) => window.scaena.source() === s, original, { timeout: 30000 }).then(() => true, () => false);
  check(back, "one undo puts the source back");
  await settled(page);
  check((await drawn(page)) === dusk, "and the preview is drawn in Dusk again");
  await page.waitForFunction(() => document.querySelector("#theme option:checked")?.textContent === "dusk", null, { timeout: 30000 }).catch(() => {});
  check((await offered(page)).includes("In the bundle/dusk*"), "the picker shows Dusk again");

  // The torture deck: Dusk lacks the roles it uses, and is refused.
  const torture = await open("/tests/fixtures/torture.scaena");
  const before = await source(torture);
  const was = await offered(torture);
  await torture.selectOption("#theme", "ships:Dusk");
  const refused = await torture
    .waitForFunction(() => document.querySelector("#status").textContent.includes("refused"), null, { timeout: 60000 })
    .then(() => true, () => false);
  const said = await status(torture);
  check(refused && said.includes("Dusk refused: the deck would not validate in it") && said.includes("more"), `Dusk is refused, with why: ${said}`);
  check((await source(torture)) === before, "the torture deck's source is as it was");
  const now = await offered(torture);
  check(JSON.stringify(now) === JSON.stringify(was), `and the picker too: ${now.join(", ")} (was ${was.join(", ")})`);
} catch (e) {
  failures.push(`error: ${e.message}`);
} finally {
  await browser.close();
  server.close();
}
if (failures.length) {
  console.log(`\n${failures.length} failed:\n${failures.map((f) => `  ${f}`).join("\n")}`);
  process.exit(1);
}

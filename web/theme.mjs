// PLAN 2.39 check: a theme chosen in the editor, in headless Chromium (serve.mjs), the CPU
// painting.
//
//   node web/theme.mjs     (after `just web`; from the repository's root)
//
// - On the revenue example, the theme picker offers the deck's Dusk, chosen, and the themes built
//   in. Daybreak chosen re-themes the deck: its source names `themes/daybreak.theme.json`, the
//   preview is drawn in it, the status says how many issues are new, and the picker offers it from
//   the deck now. One undo puts Dusk back, drawn as before.
// - On the torture deck, Dusk is refused: the deck uses roles Dusk lacks. The status says why, and
//   the source and the picker are as they were.
// Exits 1 on any failure.
import { createHash } from "node:crypto";
import { launch, retheme, serve } from "./serve.mjs";

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
    await page.waitForFunction(() => window.scaena.themeChoices().length > 0, null, { timeout: 30000 });
    return page;
  };
  const source = (page) => page.evaluate(() => window.scaena.source());
  const status = (page) => page.evaluate(() => document.querySelector("#status").textContent);
  const offered = (page) =>
    page.evaluate(() => window.scaena.themeChoices().map((c) => `${c.family}/${c.label}${c.current ? "*" : ""}`));
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
    first.includes("In this deck/Dusk*") &&
      ["Stage/Dusk", "Executive/Daybreak", "Analyst/Ember", "Editorial/Salon"].every((t) => first.includes(t)),
    `the picker offers the deck's Dusk, chosen, and the themes built in, by family: ${first.join(", ")}`,
  );

  // Theme… is a button: ↓ on it re-themes nothing, and in its dialog the arrow keys move between
  // the themes and Cancel leaves the deck as it was; Re-theme alone re-themes (lift-up USE-3,
  // WCAG 3.2.2). A select re-themed at each press.
  await page.locator("#theme").focus();
  await page.keyboard.press("ArrowDown");
  await page.waitForTimeout(300);
  check((await source(page)) === original && !(await status(page)).startsWith("Re-theming"), "↓ on Theme… re-themes nothing");
  await page.click("#theme");
  await page.waitForSelector("#retheming[open]", { timeout: 10000 });
  check(await page.evaluate(() => document.querySelector("#retheming input:checked")?.value === "path:themes/dusk.theme.json"), "the dialog opens on the deck's Dusk");
  check(await page.evaluate(() => document.querySelector("#retheming-apply").disabled), "with Re-theme off, since the deck is in it");
  await page.locator("#retheming input:checked").focus();
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  await page.waitForTimeout(300);
  check((await source(page)) === original, "the arrow keys move between the themes and re-theme nothing");
  check(await page.evaluate(() => !document.querySelector("#retheming-apply").disabled), "and Re-theme is on, another chosen");
  await page.keyboard.press("Escape");
  await page.waitForTimeout(300);
  check((await source(page)) === original && (await page.evaluate(() => !document.querySelector("#retheming").open)), "Escape leaves the deck as it was");
  check(await page.evaluate(() => document.querySelector("#theme").textContent === "Theme: Dusk"), `the button names the deck's theme: ${await page.evaluate(() => document.querySelector("#theme").textContent)}`);

  // Daybreak, which ships.
  await retheme(page, "ships:Daybreak");
  const named = await page
    .waitForFunction(() => window.scaena.source().includes('theme:"themes/daybreak.theme.json"'), null, { timeout: 60000 })
    .then(() => true, () => false);
  check(named, "Daybreak chosen, the source names it");
  check(/^Theme Daybreak · (no new issues|\d+ new issues?)/.test(await status(page)), `the status says what it found: ${await status(page)}`);
  await settled(page);
  const day = await drawn(page);
  check(day !== dusk, "the preview is drawn in Daybreak");
  const after = await offered(page);
  check(after.includes("In this deck/Daybreak*") && after.includes("In this deck/Dusk"), `the deck holds it now, chosen: ${after.join(", ")}`);

  // One undo: Dusk again, drawn as it was.
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+z");
  const back = await page.waitForFunction((s) => window.scaena.source() === s, original, { timeout: 30000 }).then(() => true, () => false);
  check(back, "one undo puts the source back");
  await settled(page);
  check((await drawn(page)) === dusk, "and the preview is drawn in Dusk again");
  await page.waitForFunction(() => window.scaena.themeChoices().find((c) => c.current)?.label === "Dusk", null, { timeout: 30000 }).catch(() => {});
  check((await offered(page)).includes("In this deck/Dusk*"), "the picker shows Dusk again");

  // The trails example in Daybreak: the storm slide's words would be dark on its dark photo, so the
  // re-theme sets each in Daybreak's color that reads there, and says so (PLAN 2.82).
  const trails = await open("/docs/examples/trails.deck.json");
  await retheme(trails, "ships:Daybreak");
  const recolored = await trails
    .waitForFunction(() => document.querySelector("#status").textContent.startsWith("Theme Daybreak"), null, { timeout: 60000 })
    .then(() => true, () => false);
  const told = await status(trails);
  check(recolored && told.includes("2 texts recolored to read on what lies behind them"), `the storm slide's two texts are recolored, and it says so: ${told}`);
  await settled(trails);
  const storm = (await source(trails)).match(/\nstate storm [\s\S]*?(?=\nstate |$)/)?.[0] ?? "";
  const surfaced = storm.match(/style:\{color: surface\}/g)?.length ?? 0;
  check(surfaced === 2, `each is set in Daybreak's surface, in the source: ${surfaced} of 2`);
  const faint = await trails.evaluate(() =>
    (window.scaena.last()?.findings ?? []).filter((f) => f.code === "E110" || f.code === "E111").map((f) => `${f.code} ${f.node} in ${f.state}`),
  );
  check(faint.length === 0, `no text is left too faint to read: ${faint.join(", ") || "none"}`);

  // The torture deck: Dusk lacks the roles it uses, and is refused.
  const torture = await open("/tests/fixtures/torture.scaena");
  const before = await source(torture);
  const was = await offered(torture);
  await retheme(torture, "ships:Dusk");
  const refused = await torture
    .waitForFunction(() => document.querySelector("#status").textContent.includes("not taken"), null, { timeout: 60000 })
    .then(() => true, () => false);
  const said = await status(torture);
  check(refused && said.includes("Dusk not taken: the deck would not open in it") && said.includes("more") && !/\b[EWI]\d{3}\b/.test(said), `Dusk is refused, with why, in sentences: ${said}`);
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

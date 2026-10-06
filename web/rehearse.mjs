// PLAN 2.63 check: a rehearsal in the editor, in headless Chromium (serve.mjs), the CPU painting,
// on the revenue example (four states: intro, revenue, mix, close).
//
//   node web/rehearse.mjs     (after `just web`; from the repository's root)
//
// - Rehearse plays the deck from its first state, a state's cue then the state at rest; → goes on
//   and ← goes back; the bar says where the rehearsal is.
// - Escape stops it. The table says what each state took, a state shown twice both times, its cue,
//   and the hold the time keeps (the time less the cue, to a tenth of a second); a state not
//   reached keeps its hold. Nothing changes the deck until Keep.
// - Keep makes the holds one patch: each state reached holds what it kept, and ⌘Z takes them all
//   back in one step; ⇧⌘Z makes them again.
// - Discard keeps nothing.
// - axe-core finds nothing against WCAG 2.1 AA with the bar shown, or the table.
// Exits 1 on any failure.
import { createRequire } from "node:module";
import { launch, serve } from "./serve.mjs";

const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};
/** How far a time the page kept may be from the time the test waited, ms. */
const SLACK = 300;

const axe = createRequire(import.meta.url).resolve("axe-core/axe.min.js");
const site = await serve();
const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1 });
  const page = await context.newPage();
  page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
  page.on("console", (m) => m.type() === "error" && failures.push(`console: ${m.text()}`));
  await page.goto(`${site.origin}/web/dist/editor.html?painter=cpu&bundle=/docs/examples/revenue.deck.json`);
  await page.waitForFunction(() => window.scaena?.last()?.valid, null, { timeout: 120000 });
  await page.addScriptTag({ path: axe });
  const audit = () =>
    page.evaluate(async () => {
      const result = await window.axe.run(document, { runOnly: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"] });
      return result.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
    });

  const source = () => page.evaluate(() => window.scaena.source());
  const at = () => page.evaluate(() => window.scaena.at().index);
  const slots = await page.evaluate(() => window.scaena.last().slots.map((s) => ({ state: s.state, span: s.span, hold: s.hold })));
  check(slots.map((s) => s.state).join(",") === "intro,revenue,mix,close", `the revenue example: ${slots.map((s) => s.state).join(", ")}`);
  const was = await source();

  const press = async (key, ms) => {
    await page.waitForTimeout(ms);
    await page.keyboard.press(key);
  };
  const rehearse = async () => {
    await page.click("#rehearse-open");
    await page.waitForFunction(() => !document.querySelector("#rehearsing").hidden, null, { timeout: 10000 }).catch(() => {});
  };
  const stopped = () => page.waitForFunction(() => document.querySelector("#rehearsed").open, null, { timeout: 10000 }).catch(() => {});

  // Rehearse: the first state, its cue played; the bar, and the table, audited; Discard keeps
  // nothing.
  await rehearse();
  check(!(await page.locator("#rehearsing").isHidden()), "Rehearse shows its bar");
  check((await at()) === 0, "and plays from the first state");
  const where = await page.locator("#rehearsing [data-where]").textContent();
  check(where.startsWith("intro (1 of 4)"), `the bar says where it is: ${where}`);
  const found = await audit();
  check(!found.length, `axe finds nothing with the bar shown${found.length ? `:\n    ${found.join("\n    ")}` : ""}`);
  await press("ArrowRight", 100);
  await page.waitForFunction(() => window.scaena.at().index === 1, null, { timeout: 5000 }).catch(() => {});
  check((await at()) === 1, "→ goes on to the next state");
  await press("ArrowLeft", 100);
  await page.waitForFunction(() => window.scaena.at().index === 0, null, { timeout: 5000 }).catch(() => {});
  check((await at()) === 0, "← goes back");
  await press("Escape", 100);
  await stopped();
  check(await page.evaluate(() => document.querySelector("#rehearsed").open), "Escape stops it, and shows what each state took");
  check(await page.locator("#rehearsing").isHidden(), "the bar is gone");
  const inTable = await audit();
  check(!inTable.length, `axe finds nothing with the table shown${inTable.length ? `:\n    ${inTable.join("\n    ")}` : ""}`);
  await page.locator("#rehearsed button[value=cancel]").click();
  await page.waitForTimeout(500);
  check((await source()) === was, "Discard keeps nothing");

  // A rehearsal timed: on, on, back, on, each after a wait; Escape stops.
  await rehearse();
  await press("ArrowRight", 900); // intro: 0.9 s
  await press("ArrowRight", 2000); // revenue: 2.0 s
  await press("ArrowLeft", 600); // mix: 0.6 s
  await press("ArrowRight", 700); // revenue again: 0.7 s
  await press("Escape", 1100); // mix again: 1.1 s
  await stopped();
  check((await source()) === was, "nothing has changed the deck until Keep");

  const kept = await page.evaluate(() => window.scaena.rehearsal.kept());
  const by = Object.fromEntries(kept.map((k) => [k.state, k]));
  const near = (got, want) => Math.abs(got - want) <= SLACK;
  check(near(by.intro.spent, 900), `intro took 0.9 s: ${by.intro.spent.toFixed(0)} ms`);
  check(near(by.revenue.spent, 2700), `revenue took 2.0 s and 0.7 s: ${by.revenue.spent.toFixed(0)} ms`);
  check(near(by.mix.spent, 1700), `mix took 0.6 s and 1.1 s: ${by.mix.spent.toFixed(0)} ms`);
  check(by.close.spent === 0 && by.close.keeps === undefined, "close was not reached, and keeps its hold");
  const holds = kept.filter((k) => k.keeps !== undefined);
  check(
    holds.every((k) => k.keeps === Math.max(0, Math.round((k.spent - k.span) / 100) * 100)),
    `each keeps its time less its cue, to a tenth of a second: ${holds.map((k) => `${k.state} ${k.keeps}`).join(", ")}`,
  );
  const rows = await page.locator("#rehearsed tbody tr").evaluateAll((trs) => trs.map((tr) => [...tr.children].map((c) => c.textContent)));
  check(rows.length === 4 && rows[3][1] === "not reached" && rows[3][3] === "as it is", `the table: ${JSON.stringify(rows)}`);

  // Keep: one patch of the holds; ⌘Z takes them back in one step; ⇧⌘Z makes them again.
  const introHold = slots[0].hold;
  await page.click("#rehearsed-keep");
  await page.waitForFunction((h) => window.scaena.last()?.valid && window.scaena.last().slots[0].hold !== h, introHold, { timeout: 30000 }).catch(() => {});
  const held = await page.evaluate(() => window.scaena.last().slots.map((s) => ({ state: s.state, hold: s.hold })));
  const holdOf = (state) => held.find((h) => h.state === state)?.hold;
  check(holds.every((k) => holdOf(k.state) === k.keeps), `Keep: each state reached holds what it kept: ${JSON.stringify(held)}`);
  check(holdOf("close") === slots.find((s) => s.state === "close").hold, "and close keeps its hold");
  const keptSource = await source();
  await page.locator(".cm-content").focus();
  await page.keyboard.press("Control+z");
  await page.waitForFunction((w) => window.scaena.source() === w, was, { timeout: 30000 }).catch(() => {});
  check((await source()) === was, "⌘Z takes every hold back in one step");
  await page.keyboard.press("Control+Shift+z");
  await page.waitForFunction((k) => window.scaena.source() === k, keptSource, { timeout: 30000 }).catch(() => {});
  check((await source()) === keptSource, "⇧⌘Z makes them again");
  await context.close();
} catch (e) {
  failures.push(String(e));
} finally {
  await browser.close();
  site.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "a rehearsal keeps each state's time, and Keep makes it the deck's holds in one step");
process.exit(failures.length ? 1 : 0);

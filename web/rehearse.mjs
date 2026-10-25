// PLAN 2.63 check: a rehearsal in the editor, in headless Chromium (serve.mjs), the CPU painting,
// on the revenue example (four states: intro, revenue, mix, close).
//
//   node web/rehearse.mjs     (after `just web`; from the repository's root)
//
// - Rehearse plays the deck from the slide shown, a state's cue then the state at rest; → goes on
//   and ← goes back; P pauses its clock; the bar says where the rehearsal is.
// - Escape stops it. The table says what each state took, a state shown twice both times, its cue,
//   and the hold the time keeps (the time less the cue, to a tenth of a second); a state not
//   reached keeps its hold. Nothing changes the deck until Keep.
// - Keep makes the holds one patch: each state reached holds what it kept, and ⌘Z takes them all
//   back in one step; ⇧⌘Z makes them again.
// - Discard keeps nothing.
// - axe-core finds nothing against WCAG 2.1 AA with the bar shown, or the table.
// Exits 1 on any failure.
import { axeInto, launch, serve } from "./serve.mjs";

const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};
/** How far a time the page kept may be from the time the test waited, ms. */
const SLACK = 300;

const site = await serve();
const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1 });
  const page = await context.newPage();
  page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
  page.on("console", (m) => m.type() === "error" && failures.push(`console: ${m.text()}`));
  await page.goto(`${site.origin}/web/dist/editor.html?painter=cpu&source=open&bundle=/docs/examples/revenue.deck.json`);
  await page.waitForFunction(() => window.scaena?.last()?.valid, null, { timeout: 120000 });
  await axeInto(page);
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
  check(where.startsWith("Rehearsing Slide 1 · "), `the bar says where it is: ${where}`);
  // Under it, the state's notes as a speaker reads them, and the next slide's name, as the
  // presenter view shows them (SPEC §2.6, lift-up USE-3): intro's own, then revenue's beat's.
  const noted = () => page.evaluate(() => ({ next: document.querySelector("#rehearsal-notes [data-next]").textContent, notes: document.querySelector("#rehearsal-notes [data-notes]").textContent }));
  await page.waitForFunction(() => document.querySelector("#rehearsal-notes [data-notes]").textContent, null, { timeout: 5000 }).catch(() => {});
  const first = await noted();
  check(
    !(await page.locator("#rehearsal-notes").isHidden()) && first.notes === "Open on the title. Don't talk over the build." && first.next === "Next: Slide 2",
    `under the bar, the slide's notes and the next slide's name: ${JSON.stringify(first)}`,
  );
  const found = await audit();
  check(!found.length, `axe finds nothing with the bar shown${found.length ? `:\n    ${found.join("\n    ")}` : ""}`);
  await press("ArrowRight", 100);
  await page.waitForFunction(() => window.scaena.at().index === 1, null, { timeout: 5000 }).catch(() => {});
  check((await at()) === 1, "→ goes on to the next state");
  await page.waitForFunction(() => document.querySelector("#rehearsal-notes [data-notes]").textContent.startsWith("Pause on the Q3 bar"), null, { timeout: 5000 }).catch(() => {});
  const second = await noted();
  check(second.notes.startsWith("Pause on the Q3 bar before the build.") && second.next === "Next: Slide 2, step 2", `with revenue's notes, its beat's, and the next: ${JSON.stringify(second)}`);
  await press("ArrowLeft", 100);
  await page.waitForFunction(() => window.scaena.at().index === 0, null, { timeout: 5000 }).catch(() => {});
  check((await at()) === 0, "← goes back");
  await press("Escape", 100);
  await stopped();
  check(await page.evaluate(() => document.querySelector("#rehearsed").open), "Escape stops it, and shows what each state took");
  check(await page.locator("#rehearsing").isHidden(), "the bar is gone");
  check(await page.locator("#rehearsal-notes").isHidden(), "and so are the notes");
  const inTable = await audit();
  check(!inTable.length, `axe finds nothing with the table shown${inTable.length ? `:\n    ${inTable.join("\n    ")}` : ""}`);
  await page.locator("#rehearsed button[value=cancel]").click();
  await page.waitForTimeout(500);
  check((await source()) === was, "Discard keeps nothing");

  // From the slide shown, and paused (lift-up 10-25b USE-2): stopped on Slide 2, the canvas shows
  // it, and the next rehearsal begins there; P pauses the clock, which counts nothing until P goes
  // on with it.
  await rehearse();
  await press("ArrowRight", 100);
  await page.waitForFunction(() => window.scaena.at().index === 1, null, { timeout: 5000 }).catch(() => {});
  await press("Escape", 100);
  await stopped();
  await page.locator("#rehearsed button[value=cancel]").click();
  await page.waitForTimeout(300);
  await rehearse();
  check((await at()) === 1, `a rehearsal begins at the slide shown: ${await at()}`);
  const bar = () => page.locator("#rehearsing [data-where]").textContent();
  check((await bar()).startsWith("Rehearsing Slide 2 · "), `the bar says so: ${await bar()}`);
  await press("p", 300);
  check(await page.evaluate(() => window.scaena.rehearsal.paused()), "P pauses it");
  const pausedAt = await bar();
  await page.waitForTimeout(1200);
  check(pausedAt.startsWith("Paused on Slide 2 · ") && (await bar()) === pausedAt, `and its clock stands: ${pausedAt} → ${await bar()}`);
  check((await page.locator("#rehearsing [data-pause]").getAttribute("aria-pressed")) === "true", "Pause reads as pressed");
  await page.click("#rehearsing [data-pause]");
  check(!(await page.evaluate(() => window.scaena.rehearsal.paused())), "Pause again goes on with it");
  await press("Escape", 400);
  await stopped();
  const paused = await page.evaluate(() => window.scaena.rehearsal.kept());
  const revenue = paused.find((k) => k.state === "revenue");
  check(
    paused[0].spent === 0 && revenue.spent > 300 && revenue.spent < 300 + 400 + SLACK,
    `the slide before was not rehearsed, and the pause counts toward none: ${paused.map((k) => `${k.state} ${k.spent.toFixed(0)}`).join(", ")}`,
  );
  await page.locator("#rehearsed button[value=cancel]").click();
  await page.waitForTimeout(300);
  // Back to the first, for what follows.
  await page.selectOption("#state", "intro");
  await page.waitForFunction(() => window.scaena.at().index === 0, null, { timeout: 5000 }).catch(() => {});

  // A second Escape keeps the timings safe (lift-up USE-2): Escape stops the run and opens the
  // results; Escape on them is held off once, the status saying they stay; the next closes them;
  // and Rehearsal Results… in the palette opens them again, where Keep keeps the timings.
  const status = () => page.locator("#status").textContent();
  const open = () => page.evaluate(() => document.querySelector("#rehearsed").open);
  await rehearse();
  await press("ArrowRight", 1500);
  await press("Escape", 100);
  await stopped();
  check(await open(), "Escape stops a run, and opens the results");
  await press("Escape", 100);
  await page.waitForTimeout(200);
  check(await open(), "Escape on the results is held off once");
  check((await status()).startsWith("Escape again closes the results, which stay"), `and the status says they stay: ${await status()}`);
  await press("Escape", 100);
  await page.waitForFunction(() => !document.querySelector("#rehearsed").open, null, { timeout: 5000 }).catch(() => {});
  check(!(await open()), "Escape again closes them");
  check((await source()) === was, "nothing kept");
  const took = await page.evaluate(() => window.scaena.rehearsal.kept());
  check(took.length === slots.length && took.some((k) => k.spent > 0), `the results stay: ${took.filter((k) => k.spent > 0).length} state(s) reached`);
  check((await status()).includes("Rehearsal Results…"), `the status says how to see them again: ${await status()}`);
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+k");
  await page.waitForFunction(() => document.querySelector("#palette").open, null, { timeout: 10000 });
  await page.keyboard.type("Rehearsal Results");
  await page.keyboard.press("Enter");
  await stopped();
  check(await open(), "Rehearsal Results… in the palette opens them again");
  const again = await page.locator("#rehearsed tbody tr").count();
  check(again === slots.length, `with every state's row: ${again}`);
  const reached = took.filter((k) => k.keeps !== undefined && k.keeps !== k.hold);
  await page.click("#rehearsed-keep");
  await page
    .waitForFunction((r) => window.scaena.last()?.valid && r.every((k) => window.scaena.last().slots.find((s) => s.state === k.state)?.hold === k.keeps), reached, { timeout: 30000 })
    .catch(() => {});
  const kept2 = await page.evaluate(() => window.scaena.last().slots.map((s) => ({ state: s.state, hold: s.hold })));
  check(
    reached.length > 0 && reached.every((k) => kept2.find((h) => h.state === k.state)?.hold === k.keeps),
    `Keep from there keeps the timings: ${JSON.stringify(kept2)}, from ${JSON.stringify(reached)}: ${await status()}`,
  );
  await page.locator(".cm-content").focus();
  await page.keyboard.press("Control+z");
  await page.waitForFunction((w) => window.scaena.source() === w, was, { timeout: 30000 }).catch(() => {});
  check((await source()) === was, "and ⌘Z takes them back");

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
  check(
    rows.map((r) => r[0]).join() === "Slide 1,Slide 2,Slide 2, step 2,Slide 3" && rows[3][1] === "Not reached" && rows[3][3] === "As it is",
    `the table, each slide as a person counts it: ${JSON.stringify(rows)}`,
  );

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

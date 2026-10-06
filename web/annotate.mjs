// PLAN 2.67 check: a chart annotated from its marks, in the editor in headless Chromium
// (serve.mjs), the CPU painting, on the revenue example's `rev` chart in the `revenue` state: a bar
// for each of `q3`'s twelve rows, by quarter and product.
//
//   node web/annotate.mjs     (after `just web`; from the repository's root)
//
// - The chart selected, a click on a bar picks it, and the canvas outlines it; its menu offers to
//   highlight it and its series, to call it out, to rule its value, and to band from it.
// - Highlight: one `annotate`, on the chart (no state sets its annotations); the bar is then
//   picked out by it, its menu takes the highlight off, and ⌘Z takes it back.
// - Call out: words typed in a field over the bar; Enter makes a callout on it with them. A click
//   on the callout selects it; dragged onto another bar, it stands there; a double click changes
//   its words; Delete takes it away, and the chart stays.
// - Rule its value; a band from one quarter's bar to another quarter's.
// - "Only in this state" keeps an annotation to the state shown.
// - Escape lets go of the mark picked, and the chart stays selected.
// - The keys sheet lists what annotates; axe-core finds nothing against WCAG 2.1 AA with a mark
//   picked and its menu open.
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
  await page.goto(`${site.origin}/web/dist/editor.html?painter=cpu&bundle=/docs/examples/revenue.deck.json`);
  await page.waitForFunction(() => window.scaena?.last()?.valid, null, { timeout: 120000 });
  await page.selectOption("#state", "revenue");
  await page.waitForFunction(() => window.scaena.canvas.boxed() === "revenue", null, { timeout: 30000 }).catch(() => {});

  const source = () => page.evaluate(() => window.scaena.source());
  /** The source's `annotations:[…]`, the chart's own first, then each state's: each where the
   * printer put it, on a line of its own or after the props before it. */
  const annotations = async () =>
    (await source()).split("\n").flatMap((line) => {
      const start = line.indexOf("annotations:[");
      if (start < 0) return [];
      let depth = 0;
      let quoted = false;
      for (let i = start + "annotations:".length; i < line.length; i++) {
        const c = line[i];
        if (quoted) {
          if (c === "\\") i++;
          else if (c === '"') quoted = false;
        } else if (c === '"') quoted = true;
        else if (c === "[" || c === "{") depth++;
        else if ((c === "]" || c === "}") && --depth === 0) return [line.slice(start, i + 1)];
      }
      return [line.slice(start)];
    });
  /** Wait for the source to say `text`, or stop saying it. */
  const says = (text, yes = true) =>
    page.waitForFunction(([t, y]) => window.scaena.source().includes(t) === y, [text, yes], { timeout: 30000 }).catch(() => {});
  /** The middle of `rect`, canvas units, on the page. */
  const onPage = ([x, y]) =>
    page.evaluate(
      ([x, y]) => {
        const r = document.querySelector("#overlay").getBoundingClientRect();
        const [w, h] = window.scaena.canvas.size();
        return [r.left + (x / w) * r.width, r.top + (y / h) * r.height];
      },
      [x, y],
    );
  const middle = ([x, y, w, h]) => [x + w / 2, y + h / 2];
  /** Row `r`'s bar, as the engine draws it now. */
  const bar = (r) => page.evaluate((r) => window.scaena.canvas.marksOf("q3", [r]).then((m) => m[0]), r);
  /** The annotation drawn over row `r`'s bar, found by going up from its top. */
  const above = (r) =>
    page.evaluate(async (r) => {
      const [m] = await window.scaena.canvas.marksOf("q3", [r]);
      const [x, y, w] = m.rect;
      for (let dy = 4; dy < 400; dy += 3) {
        const at = [x + w / 2, y - dy];
        const note = await window.scaena.canvas.noteAt(at);
        if (note?.kind === "callout") return { at, note };
      }
      return undefined;
    }, r);
  const picked = () => page.evaluate(() => window.scaena.canvas.picked()?.key.replaceAll("\u001f", " "));
  const noted = () => page.evaluate(() => window.scaena.canvas.noted());
  const menu = () => page.evaluate(() => [...document.querySelectorAll(".context-menu [role=menuitem] span")].map((s) => s.textContent));
  const rightClick = async (at) => {
    await page.mouse.click(...(await onPage(at)), { button: "right" });
    await page.waitForFunction(() => document.querySelector(".context-menu"), null, { timeout: 10000 }).catch(() => {});
  };
  const choose = (label) => page.locator(".context-menu [role=menuitem]", { hasText: label }).first().click();
  const settle = () => page.waitForTimeout(250);

  // The chart selected by a click on a bar; a second click picks the bar.
  const pro = await bar(10);
  check(pro?.key === "2026-Q3\u001fPro" && pro?.notes?.callout?.series === "Pro", `row 10 draws 2026-Q3 · Pro, with where a callout on it stands: ${JSON.stringify(pro?.notes)}`);
  await page.mouse.click(...(await onPage(middle(pro.rect))));
  await page.waitForFunction(() => window.scaena.canvas.selected() === "rev", null, { timeout: 10000 }).catch(() => {});
  check((await page.evaluate(() => window.scaena.canvas.selected())) === "rev" && (await picked()) === undefined, "a first click selects the chart");
  await settle();
  await page.mouse.click(...(await onPage(middle(pro.rect))));
  await page.waitForFunction(() => window.scaena.canvas.picked(), null, { timeout: 10000 }).catch(() => {});
  check((await picked()) === "2026-Q3 Pro", `a second picks the bar: ${await picked()}`);
  check(await page.locator("#overlay path.picked-mark").count() === 1, "and outlines it");

  // Its menu.
  await rightClick(middle(pro.rect));
  const offered = await menu();
  for (const label of ["Highlight 2026-Q3 · Pro", "Highlight Pro", "Call out 2026-Q3 · Pro…", "Rule 2026-Q3 · Pro's value", "Band from 2026-Q3 · Pro to…"]) {
    check(offered.includes(label), `its menu offers ${label}`);
  }
  check(!offered.includes("Take the highlight off 2026-Q3 · Pro"), "and no highlight to take off");
  // axe-core, a mark picked and its menu open.
  await page.addScriptTag({ path: axe });
  const found = await page.evaluate(async () => {
    const result = await window.axe.run(document, { runOnly: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"] });
    return result.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
  });
  check(!found.length, `axe finds nothing with a mark picked and its menu open${found.length ? `:\n    ${found.join("\n    ")}` : ""}`);

  // Highlight it: on the chart itself, since no state sets its annotations.
  await choose("Highlight 2026-Q3 · Pro");
  await says("kind: highlight");
  check(
    JSON.stringify(await annotations()) === JSON.stringify(['annotations:[{kind: highlight, at: {x: "2026-Q3", series: Pro}}]']),
    `highlighted, on the chart: ${JSON.stringify(await annotations())}`,
  );
  await page.waitForFunction(() => window.scaena.canvas.picked()?.notes?.highlighted?.length === 1, null, { timeout: 10000 }).catch(() => {});
  check(JSON.stringify(await page.evaluate(() => window.scaena.canvas.picked()?.notes?.highlighted)) === "[0]", "the bar is picked out by the chart's first annotation");
  await rightClick(middle(pro.rect));
  check((await menu()).includes("Take the highlight off 2026-Q3 · Pro"), "its menu now takes the highlight off");
  await choose("Take the highlight off 2026-Q3 · Pro");
  await says("kind: highlight", false);
  check(!(await source()).includes("annotations:"), "which takes it away, and the empty list with it");
  // ⌘Z puts it back; ⌘Z again, the deck as it was.
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+z");
  await says("kind: highlight");
  check((await source()).includes("kind: highlight"), "⌘Z puts the highlight back");
  await page.keyboard.press("Control+z");
  await says("kind: highlight", false);
  check(!(await source()).includes("annotations:"), "and ⌘Z again takes it back");

  // Call it out: words typed over the bar.
  await page.waitForFunction(() => window.scaena.canvas.picked(), null, { timeout: 10000 }).catch(() => {});
  if (!(await picked())) {
    await page.mouse.click(...(await onPage(middle(pro.rect))));
    await page.waitForFunction(() => window.scaena.canvas.picked(), null, { timeout: 10000 }).catch(() => {});
  }
  await rightClick(middle(pro.rect));
  await choose("Call out 2026-Q3 · Pro…");
  await page.waitForSelector("input.note-words", { timeout: 10000 }).catch(() => {});
  check((await page.locator("input.note-words").count()) === 1, "Call out asks for its words, over the bar");
  await page.keyboard.type("Pro nearly doubled");
  await page.keyboard.press("Enter");
  await says("Pro nearly doubled");
  check(
    JSON.stringify(await annotations()) === JSON.stringify(['annotations:[{kind: callout, at: {x: "2026-Q3", series: Pro}, text: "Pro nearly doubled"}]']),
    `called out on the bar: ${JSON.stringify(await annotations())}`,
  );
  check((await page.locator("input.note-words").count()) === 0, "and the field goes");

  // A click on the callout selects it.
  await page.waitForTimeout(300);
  const callout = await above(10);
  check(callout?.note?.index === 0, `the callout stands over the bar: ${JSON.stringify(callout?.note?.kind)}`);
  await page.mouse.click(...(await onPage(callout.at)));
  await page.waitForFunction(() => window.scaena.canvas.noted(), null, { timeout: 10000 }).catch(() => {});
  check((await noted())?.kind === "callout" && (await page.locator("#overlay path.noted").count()) === 1, "a click on it selects it, outlined");

  // Dragged onto 2026-Q2's Pro bar, it stands there.
  const q2 = await bar(7);
  const [from, to] = [await onPage(callout.at), await onPage(middle(q2.rect))];
  await page.mouse.move(...from);
  await page.mouse.down();
  for (let i = 1; i <= 8; i++) await page.mouse.move(from[0] + ((to[0] - from[0]) * i) / 8, from[1] + ((to[1] - from[1]) * i) / 8);
  await page.waitForFunction(() => document.querySelector("#overlay path.note-carried"), null, { timeout: 5000 }).catch(() => {});
  check((await page.locator("#overlay path.note-carried").count()) === 1, "a drag carries it");
  await page.mouse.up();
  await says('x: "2026-Q2", series: Pro');
  check(
    JSON.stringify(await annotations()) === JSON.stringify(['annotations:[{kind: callout, at: {x: "2026-Q2", series: Pro}, text: "Pro nearly doubled"}]']),
    `let go on 2026-Q2's bar, it stands there: ${JSON.stringify(await annotations())}`,
  );

  // A double click on it changes its words.
  await page.waitForTimeout(300);
  const moved = await above(7);
  check(moved?.note?.kind === "callout", "it is drawn over its new bar");
  await page.mouse.dblclick(...(await onPage(moved.at)));
  await page.waitForSelector("input.note-words", { timeout: 10000 }).catch(() => {});
  check((await page.locator("input.note-words").inputValue().catch(() => "")) === "Pro nearly doubled", "a double click asks for its words, with what it says");
  await page.locator("input.note-words").fill("Pro took off");
  await page.keyboard.press("Enter");
  await says("Pro took off");
  check((await source()).includes('text: "Pro took off"') && !(await source()).includes("nearly doubled"), "and says what was typed");

  // Delete takes it away; the chart stays.
  await page.waitForTimeout(300);
  const again = await above(7);
  await page.mouse.click(...(await onPage(again.at)));
  await page.waitForFunction(() => window.scaena.canvas.noted(), null, { timeout: 10000 }).catch(() => {});
  await page.keyboard.press("Delete");
  await says("kind: callout", false);
  check(!(await source()).includes("annotations:") && (await source()).includes("rev"), "Delete takes the callout away, and the chart stays");
  check((await page.evaluate(() => window.scaena.canvas.selected())) === "rev", "still selected");

  // Rule a bar's value; band from one quarter to another.
  const core = await bar(3);
  await page.mouse.click(...(await onPage(middle(core.rect))));
  await page.waitForFunction(() => window.scaena.canvas.picked()?.key.startsWith("2026-Q1"), null, { timeout: 10000 }).catch(() => {});
  await rightClick(middle(core.rect));
  await choose("Rule 2026-Q1 · Core's value");
  await says("kind: rule");
  check((await annotations())[0] === "annotations:[{kind: rule, at: {y: 19.8}}]", `ruled at its value: ${JSON.stringify(await annotations())}`);
  const first = await bar(0);
  await page.mouse.click(...(await onPage(middle(first.rect))));
  await page.waitForFunction(() => window.scaena.canvas.picked()?.key.startsWith("2025-Q4"), null, { timeout: 10000 }).catch(() => {});
  await rightClick(middle(first.rect));
  await choose("Band from 2025-Q4 · Core to…");
  await page.waitForFunction(() => window.scaena.canvas.banding(), null, { timeout: 10000 }).catch(() => {});
  check((await page.locator("#overlay path.band-from").count()) === 1, "a band begun outlines where it begins");
  await page.mouse.click(...(await onPage(middle((await bar(3)).rect))));
  await says("kind: band");
  check(
    (await annotations())[0] === 'annotations:[{kind: rule, at: {y: 19.8}}, {kind: band, at: {x: ["2025-Q4", "2026-Q1"]}}]',
    `banded from one quarter to the other: ${JSON.stringify(await annotations())}`,
  );

  // Kept to the state shown: the inspector's "Only in this state".
  await page.click("#tab-inspector");
  const keep = page.locator("#look [data-keep]");
  await keep.waitFor({ timeout: 10000 }).catch(() => {});
  await keep.check().catch(() => {});
  await page.mouse.click(...(await onPage(middle((await bar(11)).rect))));
  await page.waitForFunction(() => window.scaena.canvas.picked()?.key.startsWith("2026-Q3\u001fEnterprise"), null, { timeout: 10000 }).catch(() => {});
  await rightClick(middle((await bar(11)).rect));
  await choose("Highlight 2026-Q3 · Enterprise");
  await says("kind: highlight");
  const lines = await annotations();
  check(
    lines.length === 2 && lines[0].includes("kind: band") && !lines[0].includes("highlight") && lines[1].includes("kind: highlight") && lines[1].includes("kind: band"),
    `kept to the state shown: the chart's own as they were, the state's with the highlight: ${JSON.stringify(lines)}`,
  );
  await keep.uncheck().catch(() => {});

  // Escape lets go of the mark picked; the chart stays selected.
  await page.locator("#overlay").focus();
  await page.keyboard.press("Escape");
  check((await picked()) === undefined && (await page.evaluate(() => window.scaena.canvas.selected())) === "rev", "Escape lets go of the mark, and the chart stays selected");

  // The keys sheet lists what annotates.
  const keys = await page.evaluate(() => window.scaena.keys.listed().filter((k) => k.group === "Annotate").map((k) => k.keys));
  check(keys.includes("Drag a callout") && keys.includes("Delete") && keys.includes("Click a mark of the chart selected"), `the keys sheet lists what annotates: ${keys.join(", ")}`);
} finally {
  await browser.close();
  await site.close();
}
if (failures.length) {
  console.log(`\n${failures.length} failed:\n  ${failures.join("\n  ")}`);
  process.exit(1);
}
console.log("\nall ok");

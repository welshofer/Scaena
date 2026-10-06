// PLAN 2.64 check: a chart's marks and the rows of its data, in the editor in headless Chromium
// (serve.mjs), the CPU painting, on the revenue example: its `rev` chart draws a bar for each of
// `q3`'s twelve rows in the `revenue` state, and nothing in `intro`.
//
//   node web/rows.mjs     (after `just web`; from the repository's root)
//
// - With the Data tab shown, a row's cell focused outlines on the canvas what the row draws in the
//   state shown, and the line under the table says what; another row, another bar.
// - A click on a bar chooses the row it was made from in the Data tab; the canvas outlines it.
// - A double click on a bar, the Data tab hidden, opens it on the bar's row.
// - In 9:16 the outline stands where the bar does there; in a state that does not draw the row,
//   nothing is outlined, and the line says so. The Data tab hidden, nothing is outlined.
// - A state picked while a lint is under way stays shown once the lint answers.
// - axe-core finds nothing against WCAG 2.1 AA with the Data tab shown and a row chosen.
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

  /** The outlines drawn: each its node, its key, and its box on the page. */
  const outlined = () =>
    page.locator("#overlay path.row-mark").evaluateAll((paths) =>
      paths.map((p) => {
        const r = p.getBoundingClientRect();
        return { node: p.dataset.node, key: p.dataset.key, box: [r.x, r.y, r.width, r.height].map(Math.round) };
      }),
    );
  const drawn = () => page.locator("#data [data-drawn]").textContent();
  const chosen = () => page.evaluate(() => window.scaena.data.chosen());
  /** Focus row `r`'s revenue, and wait for what it draws to be outlined. */
  const focusRow = async (r) => {
    await page.locator(`#data input[data-row="${r}"][data-col="2"]`).focus();
    await page.waitForFunction((r) => /row \d/.test(document.querySelector("#data [data-drawn]").textContent) && document.querySelector("#data [data-drawn]").textContent.includes(`row ${r} `), r, { timeout: 30000 }).catch(() => {});
  };
  /** The marks outlined, canvas units. */
  const marked = () =>
    page.evaluate(() => window.scaena.canvas.rowMarks()?.marks.map((m) => ({ node: m.node, key: m.key.replaceAll("\u001f", " "), rect: m.rect })) ?? []);
  /** The middle of `rect`, canvas units, on the page as it stands now: a tab shown moves the canvas. */
  const middle = ([x, y, w, h]) =>
    page.evaluate(
      ([x, y]) => {
        const r = document.querySelector("#overlay").getBoundingClientRect();
        const [w, h] = window.scaena.canvas.size();
        return [r.left + (x / w) * r.width, r.top + (y / h) * r.height];
      },
      [x + w / 2, y + h / 2],
    );
  /** Whether the outline drawn first stands on the page where its mark is. */
  const onMark = async () => {
    const [box, mark] = [(await outlined())[0]?.box, (await marked())[0]];
    if (!box || !mark) return false;
    const [x, y] = await middle(mark.rect);
    return Math.abs(box[0] + box[2] / 2 - x) <= 2 && Math.abs(box[1] + box[3] / 2 - y) <= 2;
  };

  // The Data tab: q3 shown, nothing chosen, nothing outlined.
  await page.click("#tab-data");
  await page.waitForFunction(() => window.scaena.data.shown()?.name === "q3", null, { timeout: 30000 }).catch(() => {});
  check((await outlined()).length === 0, "the Data tab opened: nothing is outlined");

  // A row focused: its bar outlined, and said.
  await focusRow(7);
  const seven = await outlined();
  const sevenAt = (await marked())[0]?.rect;
  check(seven.length === 1 && seven[0].node === "rev" && seven[0].key === "2026-Q2 Pro", `row 7 outlines its bar: ${JSON.stringify(seven)}`);
  check(await onMark(), "where the bar stands");
  check((await drawn()) === "In this state, row 7 draws rev (2026-Q2 · Pro).", `and says so: ${await drawn()}`);
  await focusRow(10);
  const ten = await outlined();
  const tenAt = (await marked())[0]?.rect;
  check(ten.length === 1 && ten[0].key === "2026-Q3 Pro" && ten[0].box[0] > seven[0].box[0], `row 10, another bar: ${JSON.stringify(ten)}`);
  await focusRow(4);
  const four = await outlined();
  const fourAt = (await marked())[0]?.rect;
  check(four.length === 1 && four[0].key === "2026-Q1 Pro", `row 4: ${JSON.stringify(four)}`);

  // axe-core, with a row chosen and its bar outlined.
  await page.addScriptTag({ path: axe });
  const found = await page.evaluate(async () => {
    const result = await window.axe.run(document, { runOnly: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"] });
    return result.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
  });
  check(!found.length, `axe finds nothing against WCAG 2.1 AA with a row chosen${found.length ? `:\n    ${found.join("\n    ")}` : ""}`);

  // A click on row 10's bar chooses row 10, and outlines it.
  await page.mouse.click(...(await middle(tenAt)));
  await page.waitForFunction(() => JSON.stringify(window.scaena.data.chosen()) === "[10]", null, { timeout: 30000 }).catch(() => {});
  check(JSON.stringify(await chosen()) === "[10]", `a click on a bar chooses its row: ${JSON.stringify(await chosen())}`);
  check(await page.locator('#data tbody tr:nth-child(11)').evaluate((tr) => tr.classList.contains("current")), "and marks it in the table");
  await page.waitForFunction(() => document.querySelector("#overlay path.row-mark")?.dataset.key === "2026-Q3 Pro", null, { timeout: 30000 }).catch(() => {});
  check((await outlined())[0]?.key === "2026-Q3 Pro", "and outlines its bar");
  check((await page.evaluate(() => window.scaena.canvas.selected())) === "rev", "the chart is selected, as a click selects it");

  // The Data tab hidden: nothing outlined; a double click on row 7's bar opens it on row 7.
  await page.click("#tab-inspector");
  await page.waitForFunction(() => !document.querySelector("#overlay path.row-mark"), null, { timeout: 30000 }).catch(() => {});
  check((await outlined()).length === 0, "the Data tab hidden, nothing is outlined");
  await page.mouse.click(...(await middle(fourAt)));
  await page.waitForTimeout(400);
  check(JSON.stringify(await chosen()) === "[10]", "with the Data tab hidden, a click chooses nothing");
  await page.mouse.dblclick(...(await middle(sevenAt)));
  await page.waitForFunction(() => !document.querySelector("#data").hidden && JSON.stringify(window.scaena.data.chosen()) === "[7]", null, { timeout: 30000 }).catch(() => {});
  check(!(await page.locator("#data").isHidden()), "a double click on a bar opens the Data tab");
  check(JSON.stringify(await chosen()) === "[7]", `on its row: ${JSON.stringify(await chosen())}`);
  await page.waitForFunction(() => document.querySelector("#overlay path.row-mark")?.dataset.key === "2026-Q2 Pro", null, { timeout: 30000 }).catch(() => {});
  check((await outlined())[0]?.key === "2026-Q2 Pro", "and outlines it");

  // In 9:16 the bar stands elsewhere, and so does its outline.
  await page.selectOption("#format", "9:16");
  await page
    .waitForFunction((was) => window.scaena.canvas.size()[0] < window.scaena.canvas.size()[1] && JSON.stringify(window.scaena.canvas.rowMarks()?.marks[0]?.rect) !== was, JSON.stringify(sevenAt), { timeout: 30000 })
    .catch(() => {});
  await page.waitForTimeout(200);
  const tall = await marked();
  check(
    tall.length === 1 && tall[0].key === "2026-Q2 Pro" && JSON.stringify(tall[0].rect) !== JSON.stringify(sevenAt) && (await onMark()),
    `in 9:16, the outline stands where the bar does: ${JSON.stringify(tall)}`,
  );
  await page.selectOption("#format", "");

  // In the intro, nothing draws the row.
  await page.selectOption("#state", "intro");
  await page.waitForFunction(() => document.querySelector("#data [data-drawn]").textContent.startsWith("In this state, nothing"), null, { timeout: 30000 }).catch(() => {});
  check((await outlined()).length === 0, "in the intro, nothing is outlined");
  check((await drawn()) === "In this state, nothing draws row 7.", `and the line says so: ${await drawn()}`);
  // The format put back asked for a lint, which answers after the intro was picked: the intro stays.
  await page.waitForTimeout(1500);
  const after = await page.evaluate(() => [document.querySelector("#state").value, window.scaena.canvas.boxed(), window.scaena.at().index]);
  check(JSON.stringify(after) === '["intro","intro",0]', `the intro stays shown once the lint the format asked for answers: ${JSON.stringify(after)}`);
  await context.close();
} catch (e) {
  failures.push(String(e));
} finally {
  await browser.close();
  site.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "a chart's marks choose their rows, and a row chosen outlines what it draws");
process.exit(failures.length ? 1 : 0);

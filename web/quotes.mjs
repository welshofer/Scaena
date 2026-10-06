// PLAN 2.72 check: copy that quotes data, in the editor in headless Chromium (serve.mjs), the CPU
// painting, on the revenue example: the title, in `revenue`, quotes Pro's Q3 revenue from `@q3`.
//
//   node web/quotes.mjs     (after `just web`; from the repository's root)
//
// - Characters selected in a text typed in, a cell of the Data tab focused, then Quote: they
//   quote it, one `style_text` of `quote`, its figure set from the data, its row found by the
//   columns of text that tell it apart; typing goes on, and one undo takes it back.
// - The cell set: the figure is set again in the same change, and the status says so.
// - Typing into the figure makes it words: it quotes nothing.
// - axe-core finds nothing with the Data tab shown.
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
  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  const says = async (text) => {
    await page.waitForFunction((t) => document.querySelector("#status").textContent.includes(t), text, { timeout: 30000 }).catch(() => {});
    return (await status()).includes(text);
  };
  const comes = (pattern) =>
    page
      .waitForFunction((p) => new RegExp(p).test(window.scaena.source()) && !window.scaena.canvas.typed()?.sending, pattern, { timeout: 30000, polling: 50 })
      .then(() => true)
      .catch(() => false);
  const back = (to) =>
    page
      .waitForFunction((s) => window.scaena.source() === s && !window.scaena.canvas.typed()?.sending, to, { timeout: 30000, polling: 50 })
      .then(() => true)
      .catch(() => false);
  /** Type in the title, with characters `from` to `to` selected. */
  const typeIn = async (from, to) => {
    if ((await page.evaluate(() => window.scaena.canvas.typing())) !== "title") {
      await page.evaluate(() => window.scaena.canvas.select("title"));
      await page.locator("#overlay").focus();
      await page.keyboard.press("Enter");
      await page.waitForFunction(() => window.scaena.canvas.typing() === "title", null, { timeout: 30000 });
    }
    await page.evaluate(
      ([from, to]) => {
        const area = document.querySelector("textarea.typing");
        area.focus({ preventScroll: true });
        area.setSelectionRange(from, to);
        document.dispatchEvent(new Event("selectionchange"));
      },
      [from, to],
    );
  };
  /** The Data tab shown, on `@q3`, and the cell of Pro's Q3 revenue (row 10, column 2) focused. */
  const cell = () => page.locator('#data input[data-row="10"][data-col="2"]');
  const toData = async () => {
    await page.click("#tab-data");
    await cell().waitFor({ timeout: 30000 });
    await cell().focus();
  };

  const original = await source();
  // "doubled" quotes Pro's Q3 revenue.
  await typeIn(8, 15);
  await toData();
  check((await page.evaluate(() => window.scaena.canvas.typing())) === "title", "focus in the Data tab keeps the text typed in");
  await page.click("#data [data-quote]");
  const quoted = 'text: "?19\\.4"?, quote: \\{data: @q3, row: \\{quarter: "?2026-Q3"?, product: Pro\\}, column: revenue\\}';
  check(await comes(quoted), `the characters quote the cell, their row by its quarter and product, the figure set: ${(await source()).match(/title[^\n]*quote[^\n]*/)?.[0]}`);
  check(await says("quotes revenue of row 10 of @q3"), `the status says so: ${await status()}`);
  check((await page.evaluate(() => window.scaena.canvas.typing())) === "title", "typing goes on");
  await page.addScriptTag({ path: axe });
  const found = await page.evaluate(async () => {
    const result = await window.axe.run(document, { runOnly: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"] });
    return result.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
  });
  check(!found.length, `axe finds nothing with the Data tab shown${found.length ? `:\n    ${found.join("\n    ")}` : ""}`);
  await page.evaluate(() => document.querySelector("textarea.typing").focus());
  await page.keyboard.press("Control+z");
  check(await back(original), "one undo takes it back");

  // Quoted again, then the cell set: the figure follows in the same change.
  await typeIn(8, 15);
  await toData();
  await page.click("#data [data-quote]");
  await comes(quoted);
  await cell().fill("20.5");
  await cell().press("Enter");
  check(await comes('text: "?20\\.5"?, quote: \\{data: @q3'), "the cell set, the figure is set again from the data");
  check(await says("what title quotes set again"), `and the status says so: ${await status()}`);
  const findings = await page.evaluate(() => window.scaena.last()?.findings ?? []);
  check(!findings.some((f) => f.code === "W427"), "and lint finds no stale figure");

  // Typed into, the figure is words.
  // The text typed in is read again with the figure the data set.
  await page.waitForFunction(() => document.querySelector("textarea.typing")?.value.includes("20.5"), null, { timeout: 30000 }).catch(() => {});
  await typeIn(9, 10);
  await page.keyboard.type("9");
  check(await comes('\\{text: "?29\\.5"?\\}'), `typed into, the figure is words that quote nothing: ${(await source()).match(/runs:\[[^\n]*20?9?\.5[^\n]*/)?.[0]}`);
} finally {
  await browser.close();
  await site.close();
}

if (failures.length) {
  console.log(`\n${failures.length} failed:\n  ${failures.join("\n  ")}`);
  process.exit(1);
}
console.log("\nall passed");

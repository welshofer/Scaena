// PLAN 2.55 check: the editor's Data panel, in headless Chromium (serve.mjs), the CPU painting, on
// the revenue example saved to target/web-data/q3data, one of its cells made one its column does
// not read.
//
//   node web/data.mjs     (after `just web`; from the repository's root)
//
// - The Data tab shows the source as a table: its columns and their types, each row by its index,
//   and the cell that does not read marked, with why, and listed under it. axe-core finds nothing
//   against WCAG 2.1 AA in it.
// - Setting that cell to a number fixes it: the problem goes, and the deck validates.
// - A value its column does not read is refused, with why, and the cell holds what it held. So is
//   a value that would leave the deck invalid: a second Core in 2025-Q4, E103.
// - A cell set repaints the chart that reads it, and the bundle is changed, to save.
// - + Row adds a row after the one focused, − Row takes it away, and Undo and Redo put the file
//   back as each left it.
// - Saved into the browser's storage, the data file is the one `scaena data` writes for the same
//   edits, byte for byte.
// Exits 1 on any failure.
import { execFileSync } from "node:child_process";
import { createRequire } from "node:module";
import { cpSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { launch, serve } from "./serve.mjs";

const out = "target/web-data";
rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
const scaena = (args, input) =>
  execFileSync("cargo", ["run", "-q", "-p", "scaena-cli", "--locked", "--", ...args], { input, stdio: [input === undefined ? "inherit" : "pipe", "inherit", "inherit"] });
const bundle = join(out, "q3data");
scaena(["save", "docs/examples/revenue.deck.json", "--to", bundle, "--keep-fonts"]);
const csv = "data/q3-revenue.csv";
const original = readFileSync(join(bundle, csv), "utf8");
writeFileSync(join(bundle, csv), original.replace("2026-Q1,Core,19.8,1305", "2026-Q1,Core,n/a,1305"));
// What `scaena data` writes for the edits the page makes and keeps, on a copy.
const copy = join(out, "cli");
cpSync(bundle, copy, { recursive: true });
const kept = [
  { op: "set", row: 3, column: "revenue", value: "19.8" },
  { op: "set", row: 2, column: "revenue", value: "4.6" },
];
scaena(["data", copy, "q3", "--edits", "-"], JSON.stringify(kept));
const expected = readFileSync(join(copy, csv), "utf8");

const server = await serve();
const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};
const axe = createRequire(import.meta.url).resolve("axe-core/axe.min.js");

const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1600, height: 1100 }, deviceScaleFactor: 1 });
  const page = await context.newPage();
  page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
  page.on("console", (m) => m.type() === "error" && failures.push(`console: ${m.text()}`));
  await page.goto(`${server.origin}/web/dist/editor.html?painter=cpu&bundle=/${bundle}/`);
  await page.waitForFunction(() => window.scaena?.last(), null, { timeout: 120000 });
  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  const field = (row, col) => page.locator(`#data input[data-row="${row}"][data-col="${col}"]`);
  const value = (row, col) => field(row, col).inputValue();
  const rows = () => page.evaluate(() => window.scaena.data.shown()?.sheet.rows.length);
  /** Set the cell at `row`, `col` to `text`, as typed, then Enter; and wait for what the editor says. */
  const set = async (row, col, text, says) => {
    await field(row, col).fill(text);
    await field(row, col).press("Enter");
    await page.waitForFunction((s) => document.querySelector("#status").textContent.startsWith(s), says, { timeout: 60000 }).catch(() => {});
  };
  check((await page.evaluate(() => window.scaena.last().valid)) === false, "a cell its column does not read leaves the deck invalid");

  // The Data tab: the source as a table.
  await page.click("#tab-data");
  await page.waitForFunction(() => window.scaena.data.shown()?.sheet.rows.length === 12, null, { timeout: 60000 });
  const shown = await page.evaluate(() => ({
    sources: [...document.querySelectorAll("#data [data-source] option")].map((o) => o.value),
    where: document.querySelector("#data [data-where]").textContent,
    head: [...document.querySelectorAll("#data thead th")].map((th) => th.textContent),
    first: [...document.querySelectorAll('#data tbody tr:first-child input')].map((i) => i.value),
  }));
  check(
    JSON.stringify(shown.sources) === '["q3"]' && shown.where === "data/q3-revenue.csv · 12 rows",
    `it shows source q3, its file, and its rows: ${JSON.stringify(shown)}`,
  );
  check(
    JSON.stringify(shown.head) === '["row","quarterstring","productstring","revenuenumber","customersnumber"]' &&
      JSON.stringify(shown.first) === '["2025-Q4","Core","18.2","1210"]',
    `each column with its type, each row as written: ${JSON.stringify(shown.head)} ${JSON.stringify(shown.first)}`,
  );
  const marked = await page.evaluate(() => {
    const cell = document.querySelector('#data input[data-row="3"][data-col="2"]');
    return { invalid: cell.getAttribute("aria-invalid"), title: cell.title, listed: [...document.querySelectorAll("#data [data-problems] li")].map((li) => li.textContent) };
  });
  check(
    marked.invalid === "true" && marked.title.includes("`n/a` is not a number") && marked.listed.length === 1 && marked.listed[0].startsWith("revenue, row 3"),
    `the cell that does not read is marked, with why, and listed: ${JSON.stringify(marked)}`,
  );
  await page.addScriptTag({ path: axe });
  const violations = await page.evaluate(async () => (await window.axe.run(document.querySelector("#data"), { runOnly: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"] })).violations);
  check(violations.length === 0, `axe finds nothing in the Data panel${violations.length ? `: ${violations.map((v) => `${v.id} (${v.nodes.length})`).join(", ")}` : ""}`);

  // Fixed: the deck validates.
  await set(3, 2, "19.8", "revenue of row 3 set");
  await page.waitForFunction(() => window.scaena.last()?.valid, null, { timeout: 60000 }).catch(() => {});
  const fixed = await page.evaluate(() => ({
    valid: window.scaena.last().valid,
    listed: document.querySelectorAll("#data [data-problems] li").length,
    invalid: document.querySelector('#data input[data-row="3"][data-col="2"]').getAttribute("aria-invalid"),
    focus: document.activeElement?.dataset.row,
  }));
  check(fixed.valid && fixed.listed === 0 && fixed.invalid === null, `setting it to a number fixes the deck: ${JSON.stringify(fixed)}`);
  check(fixed.focus === "4", `Enter goes on to the cell below (row ${fixed.focus})`);
  check((await page.evaluate(() => window.scaena.where().dirty)) === true, "the bundle is changed, to save");

  // Refused: a value its column does not read, and one that would leave the deck invalid.
  await set(2, 2, "lots", "not made");
  const lots = await status();
  check(lots.includes("`lots` is not a number") && (await value(2, 2)) === "4.4", `a value its column does not read is refused, with why, the cell as it was: ${lots}`);
  await set(1, 1, "Core", "refused");
  const twice = await status();
  check(twice.startsWith("refused: E103") && (await value(1, 1)) === "Pro", `a second Core in 2025-Q4 is refused, E103: ${twice}`);

  // A cell set repaints the chart that reads it.
  await page.selectOption("#state", "revenue");
  await page.waitForFunction(() => window.scaena.at()?.index === 1, null, { timeout: 30000 }).catch(() => {});
  await page.waitForTimeout(300);
  const before = await page.locator("#stage").screenshot();
  await set(2, 2, "4.6", "revenue of row 2 set");
  await page.waitForTimeout(500);
  const after = await page.locator("#stage").screenshot();
  check((await value(2, 2)) === "4.6", "the cell holds what it was set to");
  check(!before.equals(after), "the chart that reads it is drawn again, changed");

  // Rows added and taken away, and the file put back.
  await field(11, 0).click();
  await page.click("#data [data-add]");
  await page.waitForFunction(() => window.scaena.data.shown()?.sheet.rows.length === 13, null, { timeout: 60000 }).catch(() => {});
  const added = await page.evaluate(() => window.scaena.data.shown()?.sheet.rows[12]);
  check((await rows()) === 13 && JSON.stringify(added) === '["","","",""]', `+ Row adds an empty row after the one focused: ${await rows()} rows, ${JSON.stringify(added)}`);
  await page.click("#data [data-remove]");
  await page.waitForFunction(() => window.scaena.data.shown()?.sheet.rows.length === 12, null, { timeout: 60000 }).catch(() => {});
  check((await rows()) === 12, `− Row takes it away: ${await rows()} rows`);
  await page.click("#data [data-undo]");
  await page.waitForFunction(() => window.scaena.data.shown()?.sheet.rows.length === 13, null, { timeout: 60000 }).catch(() => {});
  check((await rows()) === 13 && (await status()).startsWith("undone"), `Undo puts the row back: ${await rows()} rows, ${await status()}`);
  await page.click("#data [data-redo]");
  await page.waitForFunction(() => window.scaena.data.shown()?.sheet.rows.length === 12, null, { timeout: 60000 }).catch(() => {});
  check((await rows()) === 12 && (await status()).startsWith("made again"), `Redo takes it away again: ${await rows()} rows, ${await status()}`);

  // Saved into the browser's storage: the file `scaena data` writes for the same edits.
  const saved = await page.evaluate(() => window.scaena.save());
  const name = saved?.where?.name;
  const file = await page.evaluate(
    async ([name, path]) => {
      let dir = await (await navigator.storage.getDirectory()).getDirectoryHandle("bundles");
      dir = await dir.getDirectoryHandle(name);
      const parts = path.split("/");
      const last = parts.pop();
      for (const part of parts) dir = await dir.getDirectoryHandle(part);
      return (await (await dir.getFileHandle(last)).getFile()).text();
    },
    [name, csv],
  );
  check(file === expected, `saved, ${csv} is what \`scaena data\` writes for the same edits, byte for byte (${file.length} B, ${expected.length} B)`);
  check(expected.split("\n").filter((l, i) => l !== original.split("\n")[i]).length === 1, "and differs from the example in its one edited line");
} finally {
  await browser.close();
  server.close();
}

if (failures.length) {
  console.error(`\n${failures.length} failed:\n${failures.map((f) => `  ${f}`).join("\n")}`);
  process.exit(1);
}
console.log("\nall data checks pass");

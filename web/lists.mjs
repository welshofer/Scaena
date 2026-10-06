// PLAN 2.69 check: lists in a text, in the editor and the player in headless Chromium
// (serve.mjs), the CPU painting, on the torture deck's `lists` case: `list-points`, bullets at
// three levels, and `list-steps`, numbers with a paragraph that is no item.
//
//   node web/lists.mjs     (after `just web`; from the repository's root)
//
// - Typing in a list: Enter at an item's end makes another like it; Tab and Shift+Tab move it a
//   level in and out; Enter in an empty item ends the list there; ⌘⇧7 numbers the paragraph the
//   caret is in, and ⌘⇧8 bullets it. Each is one patch of the text's `list`, one step to undo.
// - A text selected, not typed in: ⌘⇧8 bullets every paragraph, and again takes them out of the
//   list; the palette's Numbered list numbers them.
// - The player's live region reads the lists as lists: `ul` and `ol`, nested by level.
// - axe-core finds nothing in the editor on the case, and the keys sheet lists the list keys.
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
  await page.goto(`${site.origin}/web/dist/editor.html?painter=cpu&bundle=/tests/fixtures/torture.scaena/deck.json`);
  await page.waitForFunction(() => window.scaena?.last()?.valid, null, { timeout: 120000 });
  await page.selectOption("#state", "lists");
  await page.waitForFunction(() => window.scaena.canvas.boxed() === "lists", null, { timeout: 30000 }).catch(() => {});

  const source = () => page.evaluate(() => window.scaena.source());
  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  /** The `list:` line under `node`'s declaration in the source, or "" where it has none. */
  const listOf = (text, node) => {
    const lines = text.split("\n");
    const at = lines.findIndex((l) => l.startsWith(`  ${node} text`));
    if (at < 0) return "";
    for (const l of lines.slice(at + 1)) {
      if (!l.startsWith("    ")) break;
      if (l.trimStart().startsWith("list:")) return l.trim();
    }
    return "";
  };
  /** Once `node`'s list reads `want`, and nothing is with the worker. */
  const lists = async (node, want, what) => {
    const ok = await page
      .waitForFunction(
        ([n, w]) => {
          const lines = window.scaena.source().split("\n");
          const at = lines.findIndex((l) => l.startsWith(`  ${n} text`));
          let found = "";
          for (const l of lines.slice(at + 1)) {
            if (!l.startsWith("    ")) break;
            if (l.trimStart().startsWith("list:")) found = l.trim();
          }
          return found === w && !window.scaena.canvas.typed()?.sending;
        },
        [node, want],
        { timeout: 30000, polling: 50 },
      )
      .then(() => true)
      .catch(() => false);
    check(ok, `${what}: ${ok ? want : listOf(await source(), node) || "(none)"}`);
  };
  const typed = () => page.evaluate(() => window.scaena.canvas.typed());
  const undo = () => page.keyboard.press("Control+z");
  const back = async (to) =>
    page
      .waitForFunction((s) => window.scaena.source() === s && !window.scaena.canvas.typed()?.sending, to, { timeout: 30000, polling: 50 })
      .then(() => true)
      .catch(() => false);

  const original = await source();
  const points = "list:[{kind: bullet}, {kind: bullet}, {kind: bullet, level: 1}, {kind: bullet, level: 2}, {kind: bullet}]";
  check(listOf(original, "list-points") === points, `the points as written: ${listOf(original, "list-points")}`);

  // axe-core on the case.
  await page.addScriptTag({ path: axe });
  const found = await page.evaluate(async () => {
    const result = await window.axe.run(document, { runOnly: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"] });
    return result.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
  });
  check(!found.length, `axe finds nothing on the case${found.length ? `:\n    ${found.join("\n    ")}` : ""}`);

  // Typed in at its end: the last point.
  await page.evaluate(() => window.scaena.canvas.select("list-points"));
  await page.locator("#overlay").focus();
  await page.keyboard.press("Enter");
  await page.waitForFunction(() => window.scaena.canvas.typing() === "list-points", null, { timeout: 30000 });
  check((await typed())?.from === (await typed())?.value.length, "Enter types in it, at its end");

  // Enter at an item's end: another like it, which takes what is typed.
  await page.keyboard.press("Enter");
  await lists("list-points", points.replace(/]$/, ", {kind: bullet}]"), "Enter makes another item like it");
  await page.keyboard.type("Six");
  await page.waitForFunction(() => window.scaena.source().includes("Back out to the first level\\nSix") && !window.scaena.canvas.typed()?.sending, null, {
    timeout: 30000,
  });
  const six = await source();
  const sixList = "list:[{kind: bullet}, {kind: bullet}, {kind: bullet, level: 1}, {kind: bullet, level: 2}, {kind: bullet}, {kind: bullet}]";
  check(listOf(six, "list-points") === sixList, `and it takes the words typed: ${listOf(six, "list-points")}`);

  // Tab: a level in; Shift+Tab: back out.
  await page.keyboard.press("Tab");
  await lists("list-points", sixList.replace(/{kind: bullet}]$/, "{kind: bullet, level: 1}]"), "Tab moves the item a level in");
  check((await typed())?.node === "list-points", "typing goes on in it");
  await page.keyboard.press("Shift+Tab");
  await lists("list-points", sixList, "Shift+Tab moves it back out");

  // Enter, then Enter in the empty item: the list ends there, and the paragraph is plain.
  await page.keyboard.press("Enter");
  await lists("list-points", sixList.replace(/]$/, ", {kind: bullet}]"), "Enter makes an empty item");
  await page.keyboard.press("Enter");
  await lists("list-points", sixList, "Enter in the empty item ends the list");
  check((await status()).includes("the list ends"), `the status says so: ${await status()}`);
  // ⌘⇧7 numbers the paragraph the caret is in; ⌘⇧8 bullets it.
  await page.keyboard.press("Control+Shift+Digit7");
  await lists("list-points", sixList.replace(/]$/, ", {kind: number}]"), "⌘⇧7 numbers the paragraph");
  await page.keyboard.press("Control+Shift+Digit8");
  await lists("list-points", sixList.replace(/]$/, ", {kind: bullet}]"), "⌘⇧8 bullets it");
  await page.keyboard.press("Control+Shift+Digit8");
  await lists("list-points", sixList, "⌘⇧8 again takes it out of the list");

  // Each a step to undo: back to the points as written.
  for (let i = 0; i < 12 && (await source()) !== original; i++) {
    await undo();
    await page.waitForTimeout(150);
  }
  check(await back(original), "undo takes each back, to the deck as it was");
  await page.keyboard.press("Escape");

  // The steps, selected and not typed in: ⌘⇧8 bullets every paragraph, and again takes them out.
  await page.evaluate(() => window.scaena.canvas.select("list-steps"));
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+Shift+Digit8");
  // Each item keeps its level.
  const steps = "list:[{kind: K}, {kind: K}, {kind: K, level: 1}, {kind: K, level: 1}, {kind: K, level: 2}, {kind: K}, {kind: K}, {kind: K}, {kind: K}]";
  await lists("list-steps", steps.replaceAll("K", "bullet"), "⌘⇧8 bullets every paragraph of a text selected, each item at its level");
  await page.keyboard.press("Control+Shift+Digit8");
  await lists("list-steps", "", "and again takes them out of the list, its `list` gone");
  // The palette's Numbered list.
  await page.keyboard.press("Control+k");
  await page.keyboard.type("Numbered list");
  await page.keyboard.press("Enter");
  await lists("list-steps", `list:[${Array(9).fill("{kind: number}").join(", ")}]`, "the palette's Numbered list numbers them");
  await page.locator("#overlay").focus();
  for (let i = 0; i < 3; i++) await undo();
  check(await back(original), "three undos: the deck as it was");

  // The keys sheet.
  await page.keyboard.press("Escape");
  await page.keyboard.press("?");
  await page.waitForSelector("#keys[open]", { timeout: 10000 }).catch(() => {});
  const sheet = await page.evaluate(() => [...document.querySelectorAll("#keys tr")].map((tr) => tr.textContent));
  for (const k of ["Bulleted list", "Numbered list", "In a list: a new item like it", "In a list: the items selected a level in"]) {
    check(sheet.some((l) => l.includes(k)), `the keys sheet lists ${k}`);
  }

  // The player reads them as lists.
  const player = await context.newPage();
  player.on("pageerror", (e) => failures.push(`player: ${e.message}`));
  await player.goto(`${site.origin}/web/dist/?painter=cpu&bundle=/tests/fixtures/torture.scaena/deck.json`);
  await player.waitForFunction(() => window.scaena?.at, null, { timeout: 120000 });
  const index = await page.evaluate(() => window.scaena.opened.states.indexOf("lists"));
  await player.evaluate((i) => window.scaena.seek(i), index);
  const read = await player
    .waitForFunction(() => document.querySelector('#reading [data-node="list-points"] ul'), null, { timeout: 30000 })
    .then(() => player.evaluate(() => [document.querySelector('#reading [data-node="list-points"]').innerHTML, document.querySelector('#reading [data-node="list-steps"]').innerHTML]))
    .catch(() => ["", ""]);
  check(/^<ul><li>Bullets[^<]*<\/li><li>A second point<ul><li>A point under it[^<]*<ul><li>And one more level in<\/li><\/ul><\/li><\/ul><\/li><li>Back out/.test(read[0]), `the points read as nested lists: ${read[0]}`);
  check(/^<ol><li>Numbers count by level<\/li><li>The count goes on<ol>.*<\/ol><\/li><li>The outer count[^<]*<\/li><\/ol><p>A plain paragraph starts it again<\/p><ol><li>One<\/li><li>Two<\/li><\/ol>$/.test(read[1]), `the steps read as numbered lists, the plain paragraph between them: ${read[1]}`);
} finally {
  await browser.close();
  await site.close();
}

if (failures.length) {
  console.log(`\n${failures.length} failed:\n  ${failures.join("\n  ")}`);
  process.exit(1);
}
console.log("\nall passed");

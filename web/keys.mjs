// PLAN 2.65 check: the keys sheet in the editor, in headless Chromium (serve.mjs), the CPU
// painting, on the revenue example.
//
//   node web/keys.mjs     (after `just web`; from the repository's root)
//
// - ? on the canvas opens the sheet; ? again closes it, and so does Escape; focus goes back where
//   it was.
// - It lists the key of every command the palette names, under the command's group, as this
//   machine shows it (Ctrl+ here), whether or not the command applies now; then what the canvas, a
//   text typed in, the strip, the layers, the cue, the find bar, the Data tab, a rehearsal, and the
//   source answer. Each group once, in order, none empty; no key and label twice in one.
// - ? typed in the source is a ?, and the sheet stays shut.
// - The palette names Show the keys, with ?, and runs it; so does the Keys button.
// - What the sheet says is what the keys do: the grid's key draws the grid, and F8 and Shift+F8 go
//   to the next finding in the source and back.
// - axe-core finds nothing against WCAG 2.1 AA with the sheet open.
// Exits 1 on any failure.
import { createRequire } from "node:module";
import { launch, serve } from "./serve.mjs";

const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};
/** The groups, in the order the sheet lists them (`GROUPS` in web/src/commands.ts). */
const ORDER = ["Select", "Edit", "Arrange", "Move and resize", "Points and corners", "Type", "Draw and insert", "See", "States", "Slides", "Layers", "The cue", "Find and replace", "The data", "Annotate", "Layouts", "Rehearse", "The source", "The editor"];

const axe = createRequire(import.meta.url).resolve("axe-core/axe.min.js");
const site = await serve();
const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1 });
  const page = await context.newPage();
  page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
  page.on("console", (m) => m.type() === "error" && failures.push(`console: ${m.text()}`));
  await page.goto(`${site.origin}/web/dist/editor.html?painter=cpu&source=open&bundle=/docs/examples/revenue.deck.json`);
  await page.waitForFunction(() => window.scaena?.last()?.valid, null, { timeout: 120000 });
  await page.selectOption("#state", "revenue");
  await page.waitForFunction(() => window.scaena.canvas.boxed() === "revenue", null, { timeout: 30000 }).catch(() => {});

  const open = () => page.evaluate(() => document.querySelector("#keys").open);
  /** The sheet as shown: each group's heading, and its rows' labels and keys. */
  const shown = () =>
    page.locator("#keys section").evaluateAll((sections) =>
      sections.map((s) => ({
        group: s.querySelector("h3").textContent,
        rows: [...s.querySelectorAll("tr")].map((tr) => ({ label: tr.querySelector("th").textContent, keys: tr.querySelector("kbd").textContent })),
      })),
    );

  // ? on the canvas: the sheet, focus in it.
  await page.locator("#overlay").focus();
  await page.keyboard.press("?");
  await page.waitForFunction(() => document.querySelector("#keys").open, null, { timeout: 5000 }).catch(() => {});
  check(await open(), "? on the canvas opens the keys");
  check(await page.evaluate(() => document.querySelector("#keys").contains(document.activeElement)), "and focus is in it");

  // What it lists: every command's key, under its group, as this machine shows it.
  const sheet = await shown();
  const groups = sheet.map((s) => s.group);
  check(JSON.stringify(groups) === JSON.stringify(ORDER.filter((g) => groups.includes(g))) && new Set(groups).size === groups.length, `each group once, in order: ${groups.join(", ")}`);
  check(groups.length === ORDER.length, `every group has keys: ${ORDER.filter((g) => !groups.includes(g)).join(", ") || "all"}`);
  check(sheet.every((s) => s.rows.length > 0), "none is empty");
  check(sheet.every((s) => new Set(s.rows.map((r) => `${r.keys}\u0000${r.label}`)).size === s.rows.length), "no key and label twice in a group");
  const commands = await page.evaluate(() => window.scaena.keys.commands());
  check(commands.length >= 25 && commands.every((c) => c.group), `every command with a key has a group: ${commands.filter((c) => !c.group).map((c) => c.label).join(", ") || `${commands.length} commands`}`);
  const listed = (group, label, keys) => sheet.some((s) => s.group === group && s.rows.some((r) => r.label === label && r.keys === keys));
  const missing = commands.filter((c) => !listed(c.group, c.label, c.keys));
  check(!missing.length, `each command's key is listed under its group: ${missing.map((c) => `${c.label} ${c.keys}`).join(", ") || "all"}`);
  check(listed("Edit", "Duplicate", "Ctrl+D") && listed("Arrange", "Ungroup", "Ctrl+Shift+G"), "as this machine shows them: Ctrl+D duplicates, Ctrl+Shift+G ungroups (whether or not it applies now)");
  check(listed("The editor", "Show the keys", "?") && listed("The editor", "Commands by name, and words for the assistant", "Ctrl+K"), "and ? and Ctrl+K");
  const rowsOf = (group) => sheet.find((s) => s.group === group)?.rows ?? [];
  check(rowsOf("Move and resize").some((r) => r.keys === "← ↑ → ↓") && rowsOf("Move and resize").some((r) => r.keys === "Shift+← ↑ → ↓"), "the canvas's arrow keys, and Shift with them");
  check(rowsOf("Rehearse").some((r) => r.keys === "→ ↓ PageDown Space Enter") && rowsOf("Rehearse").some((r) => r.keys === "← ↑ PageUp Backspace"), "a rehearsal's keys, from the keys it answers");
  check(["F8", "Shift+F8", "Ctrl+Shift+M"].every((k) => rowsOf("The source").some((r) => r.keys === k)), `the source's finding keys, from its keymap: ${rowsOf("The source").map((r) => r.keys).join(", ")}`);
  check(rowsOf("The data").length >= 4 && rowsOf("Layers").length >= 3 && rowsOf("The cue").length >= 2 && rowsOf("Find and replace").length >= 4 && rowsOf("States").length >= 4 && rowsOf("Slides").length >= 6, "the Data tab's, the layers', the cue's, the find bar's, the strip's, and the light table's");
  const count = sheet.reduce((n, s) => n + s.rows.length, 0);
  check(count === (await page.evaluate(() => window.scaena.keys.listed().length)), `the sheet shows what it lists: ${count} keys`);

  // axe-core, the sheet open.
  await page.addScriptTag({ path: axe });
  const found = await page.evaluate(async () => {
    const result = await window.axe.run(document, { runOnly: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"] });
    return result.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
  });
  check(!found.length, `axe finds nothing against WCAG 2.1 AA with the keys shown${found.length ? `:\n    ${found.join("\n    ")}` : ""}`);

  // ? again closes it, focus back on the canvas; Escape closes it too.
  await page.keyboard.press("?");
  await page.waitForFunction(() => !document.querySelector("#keys").open, null, { timeout: 5000 }).catch(() => {});
  check(!(await open()), "? again closes it");
  check(await page.evaluate(() => document.activeElement?.id === "overlay"), "and focus is back on the canvas");
  await page.keyboard.press("?");
  await page.waitForFunction(() => document.querySelector("#keys").open, null, { timeout: 5000 }).catch(() => {});
  await page.keyboard.press("Escape");
  await page.waitForFunction(() => !document.querySelector("#keys").open, null, { timeout: 5000 }).catch(() => {});
  check(!(await open()) && (await page.evaluate(() => document.activeElement?.id === "overlay")), "Escape closes it, focus back on the canvas");

  // ? in the source is a ?.
  const was = await page.evaluate(() => window.scaena.source());
  await page.locator(".cm-content").click();
  await page.keyboard.press("End");
  await page.keyboard.press("?");
  await page.waitForTimeout(300);
  check(!(await open()), "? in the source leaves the keys shut");
  check((await page.evaluate(() => window.scaena.source())).length === was.length + 1, "and types a ?");
  await page.keyboard.press("Control+z");
  await page.waitForFunction((w) => window.scaena.source() === w, was, { timeout: 30000 }).catch(() => {});

  // The palette names Show the keys, with ?, and runs it; the Keys button opens them too.
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+k");
  await page.keyboard.type("keys");
  const named = await page.locator("#palette li").first().textContent();
  check(named === "Show the keys?", `the palette names Show the keys, with ?: ${named}`);
  await page.keyboard.press("Enter");
  await page.waitForFunction(() => document.querySelector("#keys").open, null, { timeout: 5000 }).catch(() => {});
  check(await open(), "and runs it");
  await page.keyboard.press("Escape");
  await page.click("#keys-open");
  await page.waitForFunction(() => document.querySelector("#keys").open, null, { timeout: 5000 }).catch(() => {});
  check(await open(), "the Keys button opens them");
  await page.keyboard.press("Escape");
  await page.waitForFunction(() => !document.querySelector("#keys").open, null, { timeout: 5000 }).catch(() => {});

  // The grid's key, as the sheet says it, draws the grid.
  const grid = rowsOf("See").find((r) => r.label === "Show the grid")?.keys;
  await page.locator("#overlay").focus();
  await page.keyboard.press(grid.replace("Ctrl+", "Control+"));
  await page.waitForFunction(() => window.scaena.canvas.ruled(), null, { timeout: 5000 }).catch(() => {});
  check(await page.evaluate(() => window.scaena.canvas.ruled()), `${grid}, as the sheet says, shows the grid`);
  await page.keyboard.press(grid.replace("Ctrl+", "Control+"));

  // F8 and Shift+F8, as the sheet says: to the next finding in the source, and back. A headline
  // too long and forty-five words on screen are two.
  const words = Array.from({ length: 45 }, (_, i) => `word${i}`).join(" ");
  const dense = was.replace('"Revenue doubled"', '"Revenue doubled, and then some, and more"').replace('"Revenue in $M. Enterprise recognized on delivery."', `"${words}."`);
  await page.evaluate((s) => window.scaena.type(s), dense);
  await page.waitForFunction((s) => window.scaena.source() === s && window.scaena.last()?.whole, dense, { timeout: 60000, polling: 50 }).catch(() => {});
  await page.waitForFunction(() => document.querySelectorAll(".cm-lintRange").length >= 2, null, { timeout: 30000 }).catch(() => {});
  await page.locator(".cm-content").click();
  await page.evaluate(() => window.scaena.cursor(0));
  await page.keyboard.press("F8");
  const first = await page.evaluate(() => window.scaena.head());
  await page.keyboard.press("F8");
  const second = await page.evaluate(() => window.scaena.head());
  await page.keyboard.press("Shift+F8");
  const back = await page.evaluate(() => window.scaena.head());
  check(first > 0 && second > first, `F8 goes to the next finding, and the next: ${first}, ${second}`);
  check(back === first, `Shift+F8 goes back to the one before: ${back}`);
  await context.close();
} catch (e) {
  failures.push(String(e));
} finally {
  await browser.close();
  site.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "? shows every key the editor answers, by what it does, as each key does it");
process.exit(failures.length ? 1 : 0);

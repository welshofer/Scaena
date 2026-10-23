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
// - Slides by keys (PLAN 2.126): PageDown and PageUp on the slide show the next slide or step and
//   the one before, as the sheet and the palette say; Ctrl+Alt+Shift+C puts the focus on the slide
//   from the source, and Ctrl+Alt+Shift+F on the slide shown among the slides; AltGr with Shift and
//   C, a character on some keyboards, stays one. A page's first Tab is Go to the slide, which Enter
//   takes. Zoomed in, the arrows pan: the sheet says so (PLAN 2.125).
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
const ORDER = ["Select", "Edit", "Arrange", "Move and resize", "Points and corners", "Type", "Draw and insert", "See", "Slides", "Layers", "Animations", "Find and replace", "The data", "Annotate", "Layouts", "Rehearse", "The source", "The editor"];

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
  check(rowsOf("The data").length >= 4 && rowsOf("Layers").length >= 3 && rowsOf("Animations").length >= 2 && rowsOf("Find and replace").length >= 4 && rowsOf("Slides").length >= 9, "the Data tab's, the layers', the animations', the find bar's, the strip's, and the light table's");
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
  // The source with its ? does not compile: the editor takes that before the undo, so the undo's
  // compile is the last it takes, and the slides go by keys below once it has (PLAN 2.126).
  await page.waitForFunction(() => window.scaena.last()?.valid === false, null, { timeout: 30000 }).catch(() => {});
  check((await page.evaluate(() => window.scaena.last()?.valid)) === false, "the source with its ? does not compile");
  await page.keyboard.press("Control+z");
  await page.waitForFunction((w) => window.scaena.source() === w && window.scaena.last()?.valid, was, { timeout: 30000 }).catch(() => {});

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
  // Closed, and the focus back on the Keys button that opened it: the sheet gives it back as it
  // hears its close event, a task after it closes, and focus given back after the canvas is
  // focused below would take the grid's key from it.
  await page
    .waitForFunction(() => !document.querySelector("#keys").open && document.activeElement?.id === "keys-open", null, {
      timeout: 5000,
    })
    .catch(() => {});

  // The grid's key, as the sheet says it, draws the grid.
  const grid = rowsOf("See").find((r) => r.label === "Show the grid")?.keys;
  await page.locator("#overlay").focus();
  await page.keyboard.press(grid.replace("Ctrl+", "Control+"));
  await page.waitForFunction(() => window.scaena.canvas.ruled(), null, { timeout: 5000 }).catch(() => {});
  check(await page.evaluate(() => window.scaena.canvas.ruled()), `${grid}, as the sheet says, shows the grid`);
  await page.keyboard.press(grid.replace("Ctrl+", "Control+"));

  // Slides by keys (PLAN 2.126), as the sheet lists them.
  check(rowsOf("Slides").some((r) => r.label === "Show the next slide or step" && r.keys === "PageDown") && rowsOf("Slides").some((r) => r.label === "Show the slide or step before" && r.keys === "PageUp"), "the sheet lists PageDown and PageUp for the next slide and the one before");
  check(listed("The editor", "Go to the slide", "Ctrl+Alt+Shift+C") && listed("The editor", "Go to the slides and their steps", "Ctrl+Alt+Shift+F"), "and the keys that go to the slide and to the slides");
  check(rowsOf("See").some((r) => r.keys === "← ↑ → ↓" && r.label.startsWith("Zoomed in with nothing selected, pan")), "and that the arrows pan what is zoomed in (PLAN 2.125)");
  const stateShown = () => page.evaluate(() => document.querySelector("#state").value);
  const showsNow = async (state) => {
    await page.waitForFunction((s) => document.querySelector("#state").value === s, state, { timeout: 10000 }).catch(() => {});
    return (await stateShown()) === state;
  };
  // From the revenue slide, with the grid hidden again first: the slide shown follows the source's
  // cursor, which the ? typed above left wherever its click landed, and the grid's word for its key
  // would come after PageDown's.
  await page.waitForFunction(() => !window.scaena.canvas.ruled(), null, { timeout: 5000 }).catch(() => {});
  await page.selectOption("#state", "revenue");
  await page.waitForFunction(() => window.scaena.canvas.boxed() === "revenue", null, { timeout: 30000 }).catch(() => {});
  await page.locator("#overlay").focus();
  await page.keyboard.press("PageDown");
  check(await showsNow("mix"), `PageDown on the slide shows the next slide or step: ${await stateShown()}`);
  check((await page.evaluate(() => document.querySelector("#status").textContent)).includes("Slide 2, step 2"), `and says which: ${await page.evaluate(() => document.querySelector("#status").textContent)}`);
  await page.keyboard.press("PageUp");
  check(await showsNow("revenue"), `PageUp the one before: ${await stateShown()}`);
  await page.keyboard.press("Control+k");
  await page.keyboard.type("next slide");
  const nextNamed = await page.locator("#palette li").first().textContent();
  check(nextNamed === "Show the next slide or stepPageDown", `the palette names it with its key: ${nextNamed}`);
  await page.keyboard.press("Escape");
  await page.locator(".cm-content").click();
  // AltGr, which Windows gives as Ctrl and Alt, with Shift and C types Ć on a Polish keyboard: the
  // source keeps it, and the focus stays.
  const altGr = await page.evaluate(() => {
    const press = new KeyboardEvent("keydown", { key: "Ć", code: "KeyC", ctrlKey: true, altKey: true, shiftKey: true, modifierAltGraph: true, bubbles: true, cancelable: true });
    document.activeElement.dispatchEvent(press);
    return { kept: !press.defaultPrevented, inSource: Boolean(document.activeElement?.closest(".cm-editor")) };
  });
  check(altGr.kept && altGr.inSource, `AltGr+Shift+C is a character, typed where it was: ${JSON.stringify(altGr)}`);
  await page.keyboard.press("Control+Alt+Shift+C");
  check(await page.evaluate(() => document.activeElement?.id === "overlay"), "Ctrl+Alt+Shift+C puts the focus on the slide, from the source");
  await page.keyboard.press("Control+Alt+Shift+F");
  check(await page.evaluate(() => document.activeElement?.closest("#strip")?.getAttribute?.("id") === "strip" && document.activeElement.dataset.state === document.querySelector("#state").value), `Ctrl+Alt+Shift+F on the slide shown among the slides: ${await page.evaluate(() => document.activeElement?.id)}`);
  {
    // A page's first Tab: Go to the slide, which Enter takes.
    const fresh = await context.newPage();
    fresh.on("pageerror", (e) => failures.push(`page: ${e.message}`));
    await fresh.goto(`${site.origin}/web/dist/editor.html?painter=cpu&bundle=/docs/examples/revenue.deck.json`);
    await fresh.waitForFunction(() => window.scaena?.last()?.valid && window.scaena.canvas.boxes().length, null, { timeout: 120000 });
    await fresh.keyboard.press("Tab");
    const first = await fresh.evaluate(() => [document.activeElement?.id, document.activeElement?.textContent]);
    check(first[0] === "skip" && first[1] === "Go to the slide", `a page's first Tab is Go to the slide: ${first}`);
    await fresh.keyboard.press("Enter");
    check(await fresh.evaluate(() => document.activeElement?.id === "overlay"), "and Enter puts the focus on the slide");
    await fresh.close();
  }

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

// PLAN 2.50 check: the layers panel, in headless Chromium (serve.mjs), the CPU painting, on the
// revenue example, in `revenue`.
//
//   node web/layers.mjs     (after `just web`; from the repository's root)
//
// - The Layers tab lists the state shown's nodes topmost first, as the engine lists them: in
//   `revenue`, the note, the chart, and the title, and, dimmed, the subtitle and the background,
//   which leave there.
// - A click on a node's name selects it on the canvas, and a node selected on the canvas is marked
//   in the list.
// - The eye hides a node in the state shown (`hide_node`), one patch and one undo. The note enters
//   in `revenue`, so no state shows it once hidden: it stays listed, and its eye shows it again
//   (`show_node`). The eye shows the background too; the subtitle, whose slot `figure` lacks, the
//   deck refuses, and the status says why.
// - A double click on a name renames the node everywhere (`rename_node`), and Escape leaves it.
// - A node dragged before another of what holds it goes over it, by `z`, and Alt with an arrow key
//   moves the node focused one place (PLAN 2.50): each one patch, one undo.
// - axe-core finds nothing against WCAG 2.1 AA in the panel.
// Exits 1 on any failure.
import { createRequire } from "node:module";
import { launch, serve } from "./serve.mjs";

const server = await serve();
const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};
const axe = createRequire(import.meta.url).resolve("axe-core/axe.min.js");

const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1920, height: 1200 }, deviceScaleFactor: 1 });
  const page = await context.newPage();
  page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
  page.on("console", (m) => m.type() === "error" && failures.push(`console: ${m.text()}`));
  await page.goto(`${server.origin}/web/dist/editor.html?painter=cpu&bundle=/docs/examples/revenue.deck.json`);
  await page.waitForFunction(() => window.scaena?.last(), null, { timeout: 120000 });

  const source = () => page.evaluate(() => window.scaena.source());
  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  const listed = () => page.evaluate(() => window.scaena.layers.listed());
  /** Once the list is `want`, joined. */
  const lists = async (want) => {
    await page
      .waitForFunction((w) => window.scaena.layers.listed().join(" | ") === w, want.join(" | "), { timeout: 30000, polling: 50 })
      .catch(() => {});
    return (await listed()).join(" | ") === want.join(" | ");
  };
  const says = async (text) => {
    await page.waitForFunction((t) => document.querySelector("#status").textContent.includes(t), text, { timeout: 30000 }).catch(() => {});
    return (await status()).includes(text);
  };
  const back = async (to) => {
    await page.waitForFunction((s) => window.scaena.source() === s, to, { timeout: 30000, polling: 50 }).catch(() => {});
    await page.waitForTimeout(300);
    return (await source()) === to;
  };
  const row = (node) => page.locator(`#layers li[data-layer="${node}"] > .row`);
  const undo = async () => {
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+z");
  };

  // In `revenue`, with the Layers tab.
  await page.evaluate(() => window.scaena.cursor(window.scaena.source().indexOf("state revenue") + "state ".length));
  await page.waitForFunction(() => window.scaena.shown() === 1 && window.scaena.canvas.boxed() === "revenue", null, { timeout: 30000 });
  await page.locator("#tab-layers").click();
  check(await page.evaluate(() => !document.querySelector("#layers").hidden), "the Layers tab shows the panel");
  const original = await source();
  const all = ["note", "rev", "subtitle (hidden)", "title", "bg (hidden)"];
  check(await lists(all), `topmost first, those that leave dimmed: ${(await listed()).join(", ")}`);
  check(
    await page.evaluate(() => getComputedStyle(document.querySelector('#layers li[data-layer="bg"] .name')).fontStyle === "italic"),
    "a node the state does not show is set apart",
  );

  // Selecting, both ways.
  await row("rev").locator("[data-pick]").click();
  check(await page.evaluate(() => window.scaena.canvas.selected() === "rev"), "a click on a name selects the node on the canvas");
  check(await page.evaluate(() => document.querySelector('#layers li[data-layer="rev"]').getAttribute("aria-current") === "true"), "and marks it in the list");
  await page.evaluate(() => window.scaena.canvas.select("title"));
  check(
    await page
      .waitForFunction(() => document.querySelector('#layers li[data-layer="title"]')?.getAttribute("aria-current") === "true", null, { timeout: 10000 })
      .then(() => true, () => false),
    "a node selected on the canvas is marked in the list",
  );
  await row("bg").locator("[data-pick]").click();
  check(await says("bg is not shown in revenue"), `a node not shown says so: ${await status()}`);

  // The eye hides a node in the state shown, and one undo takes it back.
  await row("note").locator("[data-eye]").click();
  check(await lists(["note (hidden)", "rev", "subtitle (hidden)", "title", "bg (hidden)"]), `the eye hides the note in revenue: ${(await listed()).join(", ")}`);
  check(await says("note hidden in revenue"), `the status says so: ${await status()}`);
  check(await page.evaluate(() => !window.scaena.canvas.boxes().some((b) => b.node === "note")), "and the canvas draws it no more");
  await row("note").locator("[data-eye]").click();
  check(await lists(all), `no state shows it now, and its eye shows it again: ${(await listed()).join(", ")}`);
  await undo();
  await lists(["note (hidden)", "rev", "subtitle (hidden)", "title", "bg (hidden)"]);
  await undo();
  check(await back(original), "two undos take both back");
  check(await lists(all), "and the list with them");
  // Redone, the source the eye's patch left comes back: the canvas and the list show it, not the
  // state the undo left.
  const redo = async () => {
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+Shift+z");
  };
  await redo();
  check(await lists(["note (hidden)", "rev", "subtitle (hidden)", "title", "bg (hidden)"]), "one redo hides it again");
  await redo();
  check(await lists(all), `and another shows it: ${(await listed()).join(", ")}`);
  check(
    await page.waitForFunction(() => window.scaena.canvas.boxes()?.some((b) => b.node === "note"), null, { timeout: 10000 }).then(() => true, () => false),
    "and the canvas draws it",
  );

  // The eye of a node hidden shows it: the background, which stands in the canvas, a slot of
  // every layout. The subtitle's slot is not in `figure`'s, and the deck refuses it with why.
  await row("bg").locator("[data-eye]").click();
  check(await lists(["note", "rev", "subtitle (hidden)", "title", "bg"]), `the eye shows the background in revenue: ${(await listed()).join(", ")}`);
  check(
    await page.waitForFunction(() => window.scaena.canvas.boxes()?.some((b) => b.node === "bg"), null, { timeout: 10000 }).then(() => true, () => false),
    "and the canvas draws it",
  );
  await undo();
  check(await back(original), "one undo hides it again");
  await row("subtitle").locator("[data-eye]").click();
  check(await says("not made: the deck refuses it: slot `subtitle` is not in layout `figure`"), `one the deck refuses says why: ${await status()}`);
  check((await source()) === original, "and nothing changes");

  // A double click on a name renames the node everywhere; Escape leaves it.
  await row("note").locator("[data-pick]").dblclick();
  const field = page.locator('#layers li[data-layer="note"] input');
  check((await field.count()) === 1, "a double click on a name gives a field");
  await field.press("Escape");
  check((await page.locator("#layers input").count()) === 0 && (await source()) === original, "Escape leaves the name as it was");
  await row("note").locator("[data-pick]").dblclick();
  await page.locator('#layers li[data-layer="note"] input').fill("caption");
  await page.locator('#layers li[data-layer="note"] input').press("Enter");
  check(await lists(["caption", "rev", "subtitle (hidden)", "title", "bg (hidden)"]), `Enter renames it: ${(await listed()).join(", ")}`);
  const renamed = await source();
  // Where the deck names the node (its declaration, its choreography, where it leaves); the slot
  // it stands in keeps its name.
  const named = ["caption text", "choreo caption", "-caption"].every((n) => renamed.includes(n));
  check(named && !/note text|choreo note|-note\b/.test(renamed) && renamed.includes("at:in(note)"), "everywhere the deck names it");
  check(await says("note renamed caption"), `the status says so: ${await status()}`);
  await undo();
  check(await back(original), "one undo takes the name back");

  // Dragged before the note, the title goes over it: one `z`, one patch, one undo.
  await row("title").dragTo(row("note"), { targetPosition: { x: 40, y: 2 } });
  check(await lists(["title", "note", "rev", "subtitle (hidden)", "bg (hidden)"]), `dragged before the note, the title is listed first: ${(await listed()).join(", ")}`);
  check(await says("title moved before note"), `the status says so: ${await status()}`);
  check((await source()).includes("z:1"), "by its z");
  await undo();
  check(await back(original), "one undo takes it back");
  // Alt with ↓ moves the note focused one place down: after the chart, under it. No one z does
  // that, so the chart goes up instead: the fewest that do.
  await lists(all);
  await row("note").locator("[data-pick]").focus();
  await page.keyboard.press("Alt+ArrowDown");
  check(await lists(["rev", "note", "subtitle (hidden)", "title", "bg (hidden)"]), `Alt+↓ moves the note under the chart: ${(await listed()).join(", ")}`);
  check(await page.evaluate(() => document.activeElement?.closest("[data-layer]")?.dataset.layer === "note"), "and keeps the focus on it");
  await page.keyboard.press("Alt+ArrowDown");
  check(await says("note moved after title"), `again, under the title: ${await status()}`);
  await undo();
  await undo();
  check(await back(original), "two undos take both back");
  await row("note").locator("[data-pick]").focus();
  await page.keyboard.press("Alt+ArrowUp");
  check(await says("note is first among what holds it"), `the topmost goes no higher: ${await status()}`);

  // axe-core on the panel.
  await page.addScriptTag({ path: axe });
  const found = await page.evaluate(async () => {
    const result = await window.axe.run({ include: [["#layers"], ["#tabs"]] }, { runOnly: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"] });
    return result.violations.map((v) => `${v.id} (${v.impact}): ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
  });
  check(!found.length, `axe finds nothing against WCAG 2.1 AA in the panel${found.length ? `:\n    ${found.join("\n    ")}` : ""}`);
  check(await page.evaluate(() => window.scaena.last().valid), "the source compiles and validates");
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

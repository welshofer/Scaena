// PLAN 2.50 check: the layers panel, in headless Chromium (serve.mjs), the CPU painting, on the
// revenue example, in `revenue`, then on the torture deck's `containers` case.
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
// - A node dragged before another of what holds it goes over it, by `z`, and Alt with ↓ or ↑
//   moves the node focused one place: each one patch, one undo.
// - On the torture deck's `containers` case, a node dropped before a child of another stack goes
//   into that stack there; one dropped before a root comes out onto the canvas, in the cells
//   it stood in; one dropped on a frame's middle goes into it, first and inside its
//   padding. Alt with ← takes a node out of its container, and Alt with → puts it into the one
//   before it. A container into what it holds, or a node into a text, is refused.
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

  // Into other containers, on the torture deck's `containers` case: a stack's children, a
  // frame's, a group's, and the canvas.
  await page.goto(`${server.origin}/web/dist/editor.html?painter=cpu&bundle=/tests/fixtures/torture.scaena/deck.json`);
  await page.waitForFunction(() => window.scaena?.last(), null, { timeout: 120000 });
  const containers = await page.evaluate(() => window.scaena.opened.states.indexOf("containers"));
  await page.evaluate(() => window.scaena.cursor(window.scaena.source().indexOf("state containers") + "state ".length));
  await page.waitForFunction((i) => window.scaena.shown() === i && window.scaena.canvas.boxed() === "containers", containers, { timeout: 30000 });
  await page.locator("#tab-layers").click();
  const torture = await source();
  /** Once the list, joined, has each of `want`. */
  const has = async (...want) => {
    const all = () => window.scaena.layers.listed().join(" | ");
    await page.waitForFunction((w) => w.every((x) => window.scaena.layers.listed().join(" | ").includes(x)), want, { timeout: 30000, polling: 50 }).catch(() => {});
    const now = await page.evaluate(all);
    return want.every((w) => now.includes(w));
  };
  const box = (node) => page.evaluate((n) => window.scaena.canvas.boxes()?.find((b) => b.node === n)?.rect, node);
  /** Drop `node`'s row on `on`'s: `before` its top, `after` its bottom, or `into` its middle.
   * The list drawn as the test found it first: an undo draws it again. */
  const holding = ["stats [stat-a [stat-a-figure, stat-a-label], stat-b", "tally [tally-figure, tally-label]", "card [card-tag-label, card-tag, card-photo]"];
  const drop = async (node, on, where) => {
    await has(...holding);
    await row(on).waitFor({ state: "visible" });
    // The panel runs past the window's foot: the row dropped on is brought into view first.
    await row(on).scrollIntoViewIfNeeded();
    const { height } = await row(on).boundingBox();
    const y = where === "before" ? 2 : where === "after" ? height - 2 : height / 2;
    await row(node).dragTo(row(on), { targetPosition: { x: 40, y } });
  };
  check(await has(...holding), `each container holds its own: ${(await listed()).join(" | ")}`);

  // Dropped before a child of another stack, the tally's label goes into that stack, before it.
  await drop("tally-label", "stat-a-label", "before");
  check(await has("stat-a [stat-a-figure, tally-label, stat-a-label]", "tally [tally-figure]"), `into another stack, at the drop: ${(await listed()).join(" | ")}`);
  check(await says("tally-label moved into stat-a, before stat-a-label"), `the status says so: ${await status()}`);
  await undo();
  check(await back(torture), "one undo takes it back");

  // Dropped before the card, its tag's label comes out onto the canvas, over the card, on the
  // cells it stood in.
  const tag = await box("card-tag-label");
  await drop("card-tag-label", "card", "before");
  check(await has("card-tag-label | card [card-tag, card-photo]"), `onto the canvas, over the card: ${(await listed()).join(" | ")}`);
  check(await says("card-tag-label moved onto the canvas, before card"), `the status says so: ${await status()}`);
  const out = await page.waitForFunction((t) => {
    const b = window.scaena.canvas.boxes()?.find((x) => x.node === "card-tag-label")?.rect;
    return b && b[0] <= t[0] + t[2] && t[0] <= b[0] + b[2] && b[1] <= t[1] + t[3] && t[1] <= b[1] + b[3] && b;
  }, tag, { timeout: 10000 }).then((h) => h.jsonValue(), () => undefined);
  check(out !== undefined, `where it stood: ${JSON.stringify(tag)} → ${JSON.stringify(out)}`);
  await undo();
  check(await back(torture), "one undo takes it back");

  // Dropped on the card's middle, a mark goes into it, first among what it holds, inside its
  // padding.
  await drop("marks-dot", "card", "into");
  check(await has("card [marks-dot, card-tag-label, card-tag, card-photo]", "marks [marks-ring]"), `into the frame, first: ${(await listed()).join(" | ")}`);
  check(await says("marks-dot moved into card"), `the status says so: ${await status()}`);
  const [dot, card] = [await box("marks-dot"), await box("card")];
  check(
    dot && card && dot[0] >= card[0] && dot[1] >= card[1] && dot[0] + dot[2] <= card[0] + card[2] + 0.5 && dot[1] + dot[3] <= card[1] + card[3] + 0.5,
    `inside it: ${JSON.stringify(dot)} in ${JSON.stringify(card)}`,
  );
  await undo();
  check(await back(torture), "one undo takes it back");

  // Alt with ← takes a stat's label out of its stat, before it; Alt with → puts it into the stat
  // listed before it, last.
  await has(...holding);
  await row("stat-b-label").locator("[data-pick]").focus();
  await page.keyboard.press("Alt+ArrowLeft");
  check(await has("stat-a [stat-a-figure, stat-a-label], stat-b-label, stat-b [stat-b-figure]"), `Alt+← takes it out: ${(await listed()).join(" | ")}`);
  check(await says("stat-b-label moved into stats, before stat-b"), `the status says so: ${await status()}`);
  await row("stat-b-label").locator("[data-pick]").focus();
  await page.keyboard.press("Alt+ArrowRight");
  check(await has("stat-a [stat-a-figure, stat-a-label, stat-b-label], stat-b [stat-b-figure]"), `Alt+→ puts it into the stat before it: ${(await listed()).join(" | ")}`);
  check(await says("stat-b-label moved into stat-a, after stat-a-label"), `the status says so: ${await status()}`);
  await undo();
  await undo();
  check(await back(torture), "two undos take both back");
  await has(...holding);
  await row("case").locator("[data-pick]").focus();
  await page.keyboard.press("Alt+ArrowLeft");
  check(await says("case is on the canvas already"), `a root goes no further out: ${await status()}`);

  // What may not be: a container into what it holds, a node into what holds nothing.
  await page.evaluate(() => window.scaena.layers.restack("stats", { into: "stat-a" }));
  check(await says("a node goes into nothing it holds"), `a stack into its own child is refused: ${await status()}`);
  await page.evaluate(() => window.scaena.layers.restack("tally", { into: "case" }));
  check(await says("`case` holds nothing"), `into a text, refused: ${await status()}`);
  check((await source()) === torture, "and nothing changes");

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

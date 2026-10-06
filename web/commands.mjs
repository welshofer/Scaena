// PLAN 2.53 check: commands by name, in headless Chromium (serve.mjs), the CPU painting, on the
// revenue example, in `revenue`.
//
//   node web/commands.mjs     (after `just web`; from the repository's root)
//
// - Ctrl+K opens the palette, focus in its search box, and lists only what applies: with nothing
//   selected, no Duplicate. Escape closes it, and focus goes back to the canvas.
// - The title selected, "dup" then Enter duplicates it, as Ctrl+D does: one patch, one undo.
// - "zoom in" zooms the preview in, and "fit" shows the whole canvas again.
// - "theme daybreak" re-themes the deck in Daybreak, as the theme picker does; one undo.
// - "copy" puts the title on the clipboard, as Ctrl+C does.
// - Words no command has, last, are asked of the assistant: without a key, the Assistant tab
//   shows them in its question box, says the key is missing, and focuses its field.
// - A right click on the title selects it and offers what is done to a node: Bring to front puts
//   it in front of all, one undo. Escape closes the menu, focus back on the canvas.
// - A right click where nothing is selects nothing, and offers the canvas's own: Paste puts what
//   was copied there; Draw a rectangle arms the canvas; Insert… opens the palette on the inserts.
// - Shift+F10 on the canvas offers what is done to the node selected, from the keyboard: the arrow
//   keys move in the menu, and Enter runs what is focused.
// - A right click on a state in the strip shows it and offers Add a step, one undo; on a layer,
//   Hide it in this state, one undo; on a layer not shown, Show it, and nothing done to a node.
// - axe-core finds nothing against WCAG 2.1 AA in the palette open, nor in a menu open.
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
  await context.grantPermissions(["clipboard-read", "clipboard-write"], { origin: server.origin });
  const page = await context.newPage();
  page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
  page.on("console", (m) => m.type() === "error" && failures.push(`console: ${m.text()}`));
  await page.goto(`${server.origin}/web/dist/editor.html?painter=cpu&bundle=/docs/examples/revenue.deck.json`);
  await page.waitForFunction(() => window.scaena?.last(), null, { timeout: 120000 });

  const source = () => page.evaluate(() => window.scaena.source());
  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  const says = async (text) => {
    await page.waitForFunction((t) => document.querySelector("#status").textContent.includes(t), text, { timeout: 30000 }).catch(() => {});
    return (await status()).includes(text);
  };
  const back = async (to) => {
    await page.waitForFunction((s) => window.scaena.source() === s, to, { timeout: 30000, polling: 50 }).catch(() => {});
    await page.waitForTimeout(300);
    return (await source()) === to;
  };
  const focused = () => page.evaluate(() => document.activeElement?.id || document.activeElement?.getAttribute("role") || document.activeElement?.tagName);
  const undo = async () => {
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+z");
  };
  /** The palette opened by Ctrl+K from the canvas, `words` typed: what it lists. */
  const palette = async (words = "") => {
    await page.keyboard.press("Control+k");
    await page.waitForFunction(() => document.querySelector("#palette").open, null, { timeout: 10000 });
    if (words) await page.keyboard.type(words);
    return page.evaluate(() => window.scaena.commands.shown());
  };
  const menu = () => page.evaluate(() => [...document.querySelectorAll(".context-menu [role=menuitem] span")].map((s) => s.textContent));
  const menuOpen = () =>
    page.waitForFunction(() => document.querySelector(".context-menu"), null, { timeout: 10000 }).then(
      () => true,
      () => false,
    );
  /** Click menu item `label` in the menu open. */
  const choose = (label) => page.locator(".context-menu [role=menuitem]", { hasText: label }).first().click();
  /** The client point at the middle of `node`'s box, at the zoom shown. */
  const middle = (node) =>
    page.evaluate((n) => {
      const b = window.scaena.canvas.boxes().find((x) => x.node === n);
      const r = document.querySelector("#overlay").getBoundingClientRect();
      const [x, y, w, h] = window.scaena.canvas.view();
      const [cx, cy] = [b.rect[0] + b.rect[2] / 2, b.rect[1] + b.rect[3] / 2];
      return [r.left + ((cx - x) / w) * r.width, r.top + ((cy - y) / h) * r.height];
    }, node);
  const axeOn = async (selectors) => {
    if (!(await page.evaluate(() => !!window.axe))) await page.addScriptTag({ path: axe });
    return page.evaluate(async (s) => {
      const result = await window.axe.run({ include: s.map((x) => [x]) }, { runOnly: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"] });
      return result.violations.map((v) => `${v.id} (${v.impact}): ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
    }, selectors);
  };

  // In `revenue`.
  await page.evaluate(() => window.scaena.cursor(window.scaena.source().indexOf("state revenue") + "state ".length));
  await page.waitForFunction(() => window.scaena.shown() === 1 && window.scaena.canvas.boxed() === "revenue", null, { timeout: 30000 });
  const original = await source();

  // The palette, what applies, and Escape.
  await page.evaluate(() => window.scaena.canvas.select(undefined));
  await page.locator("#overlay").focus();
  const none = await palette("dup");
  check((await focused()) === "combobox", `Ctrl+K opens the palette, focus in its search box: ${await focused()}`);
  check(!none.includes("Duplicate"), `with nothing selected, no Duplicate: ${none.join(", ")}`);
  check(none.at(-1) === "Ask the assistant: “dup”", "the words, last, go to the assistant");
  await page.keyboard.press("Escape");
  check(await page.evaluate(() => !document.querySelector("#palette").open), "Escape closes it");
  check((await focused()) === "overlay", `and focus goes back to the canvas: ${await focused()}`);

  // Duplicate, as Ctrl+D does.
  await page.evaluate(() => window.scaena.canvas.select("title"));
  const dup = await palette("dup");
  check(dup[0] === "Duplicate", `the title selected, "dup" lists Duplicate first: ${dup.join(", ")}`);
  await page.keyboard.press("Enter");
  check(await says("title copied as"), `Enter duplicates the title: ${await status()}`);
  const copy = await page.evaluate(() => window.scaena.canvas.selected());
  check(copy !== "title" && (await source()).includes(`\n  ${copy} `), `the copy is in the source, selected: ${copy}`);
  await undo();
  check(await back(original), "one undo takes it back");

  // Zoom.
  await palette("zoom in");
  await page.keyboard.press("Enter");
  check(
    await page.waitForFunction(() => window.scaena.canvas.zoomed() > 1, null, { timeout: 10000 }).then(() => true, () => false),
    `"zoom in" zooms in: ${await page.evaluate(() => window.scaena.canvas.zoomed())}`,
  );
  await palette("fit");
  await page.keyboard.press("Enter");
  check(
    await page.waitForFunction(() => window.scaena.canvas.zoomed() === 1, null, { timeout: 10000 }).then(() => true, () => false),
    `"fit" shows the whole canvas: ${await page.evaluate(() => window.scaena.canvas.zoomed())}`,
  );

  // A theme, as the picker chooses it.
  const themed = await palette("theme daybreak");
  check(themed[0] === "Re-theme in Daybreak", `"theme daybreak" lists Re-theme in Daybreak first: ${themed.join(", ")}`);
  await page.keyboard.press("Enter");
  check(
    await page
      .waitForFunction(() => window.scaena.source().includes('theme:"themes/daybreak.theme.json"'), null, { timeout: 60000 })
      .then(() => true, () => false),
    "Enter re-themes the deck in Daybreak",
  );
  check(await says("theme Daybreak · lint finds"), `the status says what lint finds: ${await status()}`);
  await undo();
  check(await back(original), "one undo puts Dusk back");

  // Copy, as Ctrl+C does.
  await page.waitForFunction(() => window.scaena.canvas.boxed() === "revenue", null, { timeout: 30000 });
  await page.evaluate(() => window.scaena.canvas.select("title"));
  await page.waitForFunction(() => window.scaena.canvas.held(), null, { timeout: 10000 }).catch(() => {});
  await page.evaluate(() => navigator.clipboard.writeText(""));
  await palette("copy");
  await page.keyboard.press("Enter");
  const held = await page
    .waitForFunction(() => navigator.clipboard.readText().then((t) => t.startsWith("{") && t), null, { timeout: 10000 })
    .then((h) => h.jsonValue(), () => "");
  check(JSON.parse(held || "{}").node === "title", `"copy" puts the title on the clipboard: ${held.slice(0, 60)}`);

  // Words for the assistant, which has no key yet.
  const asked = await palette("make the title shorter");
  check(asked.at(-1) === "Ask the assistant: “make the title shorter”", `the words, last, go to the assistant: ${asked.join(", ")}`);
  await page.keyboard.press("ArrowUp");
  await page.keyboard.press("Enter");
  check(await page.evaluate(() => !document.querySelector("#assistant").hidden), "the Assistant tab shows");
  check(
    await page.evaluate(() => document.querySelector("#question").value === "make the title shorter"),
    "its question box holds the words",
  );
  check(
    await page
      .waitForFunction(() => document.querySelector("#transcript")?.textContent.includes("key first"), null, { timeout: 10000 })
      .then(() => true, () => false),
    "it says the key is missing",
  );
  check((await focused()) === "key", `and the key's field has the focus: ${await focused()}`);
  await page.locator("#tab-inspector").click();

  // A right click on the title.
  await page.evaluate(() => window.scaena.canvas.select(undefined));
  const [tx, ty] = await middle("title");
  await page.mouse.click(tx, ty, { button: "right" });
  check(await menuOpen(), "a right click on the title opens a menu");
  check(await page.evaluate(() => window.scaena.canvas.selected() === "title"), "and selects the title");
  const onNode = await menu();
  check(
    ["Type in it", "Duplicate", "Copy", "Bring to front", "Delete"].every((l) => onNode.includes(l)) && !onNode.includes("Paste"),
    `it offers what is done to a node: ${onNode.join(", ")}`,
  );
  check((await focused()) === "menuitem", `focus on its first item: ${await focused()}`);
  check(!(await axeOn([".context-menu"])).length, `axe finds nothing in the menu: ${(await axeOn([".context-menu"])).join("; ")}`);
  await page.keyboard.press("Escape");
  check(await page.evaluate(() => !document.querySelector(".context-menu")), "Escape closes the menu");
  check((await focused()) === "overlay", `focus back on the canvas: ${await focused()}`);
  await page.mouse.click(tx, ty, { button: "right" });
  await menuOpen();
  await choose("Bring to front");
  check(
    await page
      .waitForFunction(() => window.scaena.layers.listed()[0] === "title", null, { timeout: 30000, polling: 50 })
      .then(() => true, () => false),
    `Bring to front puts the title in front of all: ${(await page.evaluate(() => window.scaena.layers.listed())).join(", ")}`,
  );
  await undo();
  check(await back(original), "one undo takes it back");

  // A right click where nothing is: the top left corner, outside every box in `revenue`.
  const [ex, ey] = await page.evaluate(() => {
    const r = document.querySelector("#overlay").getBoundingClientRect();
    return [r.left + 6, r.top + 6];
  });
  const empty = await page.evaluate(([x, y]) => {
    const r = document.querySelector("#overlay").getBoundingClientRect();
    const [w, h] = window.scaena.canvas.size();
    const [px, py] = [((x - r.left) / r.width) * w, ((y - r.top) / r.height) * h];
    return !window.scaena.canvas.boxes().some((b) => b.draws && px >= b.rect[0] && px <= b.rect[0] + b.rect[2] && py >= b.rect[1] && py <= b.rect[1] + b.rect[3]);
  }, [ex, ey]);
  check(empty, "nothing stands at the top left corner");
  await page.mouse.click(ex, ey, { button: "right" });
  await menuOpen();
  const onCanvas = await menu();
  check(await page.evaluate(() => window.scaena.canvas.selected() === undefined), "a right click where nothing is selects nothing");
  check(
    ["Paste", "Insert…", "Draw a text", "Draw a rectangle", "Zoom in"].every((l) => onCanvas.includes(l)) && !onCanvas.includes("Duplicate"),
    `it offers the canvas's own: ${onCanvas.join(", ")}`,
  );
  const before = await source();
  await choose("Paste");
  check(await says("pasted in revenue"), `Paste puts what was copied there: ${await status()}`);
  const pasted = await page.evaluate(() => window.scaena.canvas.selected());
  check(pasted !== undefined && pasted !== "title" && (await source()) !== before, `the paste is selected: ${pasted}`);
  await undo();
  check(await back(original), "one undo takes it back");
  await page.mouse.click(ex, ey, { button: "right" });
  await menuOpen();
  await choose("Draw a rectangle");
  check(await page.evaluate(() => window.scaena.canvas.armed()?.key === "r"), "Draw a rectangle arms the canvas");
  await page.locator("#overlay").focus();
  await page.keyboard.press("Escape");
  await page.mouse.click(ex, ey, { button: "right" });
  await menuOpen();
  await choose("Insert…");
  const inserts = await page.evaluate(() => window.scaena.commands.shown());
  check(
    inserts.length > 2 && inserts.slice(0, -1).every((l) => l.startsWith("Insert ")),
    `Insert… opens the palette on the inserts: ${inserts.slice(0, 4).join(", ")}…`,
  );
  check(!(await axeOn(["#palette"])).length, `axe finds nothing in the palette: ${(await axeOn(["#palette"])).join("; ")}`);
  await page.keyboard.press("Escape");

  // From the keyboard: Shift+F10 on the canvas, the title selected.
  await page.evaluate(() => window.scaena.canvas.select("title"));
  await page.locator("#overlay").focus();
  await page.keyboard.press("Shift+F10");
  check(await menuOpen(), "Shift+F10 opens the node's menu");
  const keyed = await menu();
  // A text's own commands (Type in it, then its lists, PLAN 2.69) come before what is done to any node.
  const duplicate = keyed.indexOf("Duplicate");
  check(keyed[0] === "Type in it" && duplicate > 0, `the node's commands: ${keyed.slice(0, duplicate + 1).join(", ")}…`);
  for (let i = 0; i < duplicate; i++) await page.keyboard.press("ArrowDown");
  check(
    await page.evaluate(() => document.activeElement?.textContent.startsWith("Duplicate")),
    "↓ moves item by item, to Duplicate",
  );
  await page.keyboard.press("Enter");
  check(await says("title copied as"), `Enter runs it: ${await status()}`);
  await undo();
  check(await back(original), "one undo takes it back");

  // A state in the strip.
  const states = await page.evaluate(() => window.scaena.last().states.length);
  await page.locator('#strip li[data-state="intro"]').click({ button: "right" });
  check(await menuOpen(), "a right click on a state opens a menu");
  check(
    await page.waitForFunction(() => window.scaena.shown() === 0, null, { timeout: 10000 }).then(() => true, () => false),
    "and shows that state",
  );
  const onState = await menu();
  check(
    ["Add a step", "Add a slide", "Rename the state", "Delete the state"].every((l) => onState.includes(l)),
    `it offers what is done to a state: ${onState.join(", ")}`,
  );
  await choose("Add a step");
  check(
    await page.waitForFunction((n) => window.scaena.last().states.length === n + 1, states, { timeout: 30000 }).then(() => true, () => false),
    "Add a step adds a state",
  );
  await undo();
  check(await back(original), "one undo takes it out");

  // A layer.
  await page.evaluate(() => window.scaena.cursor(window.scaena.source().indexOf("state revenue") + "state ".length));
  await page.waitForFunction(() => window.scaena.shown() === 1 && window.scaena.canvas.boxed() === "revenue", null, { timeout: 30000 });
  await page.locator("#tab-layers").click();
  await page.waitForFunction(() => window.scaena.layers.listed().includes("note"), null, { timeout: 30000 });
  await page.locator('#layers li[data-layer="note"] > .row').click({ button: "right" });
  check(await menuOpen(), "a right click on a layer opens a menu");
  check(await page.evaluate(() => window.scaena.canvas.selected() === "note"), "and selects its node");
  const onLayer = await menu();
  check(
    ["Hide it in this state", "Rename it", "Duplicate", "Delete"].every((l) => onLayer.includes(l)),
    `it offers what is done to a layer and its node: ${onLayer.join(", ")}`,
  );
  await choose("Hide it in this state");
  check(
    await page
      .waitForFunction(() => window.scaena.layers.listed().includes("note (hidden)"), null, { timeout: 30000 })
      .then(() => true, () => false),
    "Hide it in this state hides the note there",
  );
  await undo();
  check(await back(original), "one undo shows it again");
  await page.waitForFunction(() => window.scaena.layers.listed().includes("subtitle (hidden)"), null, { timeout: 30000 });
  await page.locator('#layers li[data-layer="subtitle"] > .row').click({ button: "right" });
  await menuOpen();
  const hidden = await menu();
  check(
    hidden.includes("Show it in this state") && !hidden.includes("Duplicate"),
    `on a layer not shown: Show it, and nothing done to a node: ${hidden.join(", ")}`,
  );
  await page.keyboard.press("Escape");
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
console.log("\ncommands by name, in the palette and by a right click");

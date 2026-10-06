// PLAN 2.71 check: a theme's layouts on the canvas, in the editor in headless Chromium
// (serve.mjs), the CPU painting, on the revenue example: its `revenue` state uses Dusk's `figure`
// layout, which `mix` takes too.
//
//   node web/layouts.mjs     (after `just web`; from the repository's root)
//
// - Layout shows the state's layout as its slots, each named, on the theme's grid.
// - A slot dragged lands on the grid's tracks, one `theme_edit`: the node in it moves, in every
//   state that uses the layout; ⌘Z takes it back. A handle resizes a slot. Escape through a drag
//   leaves the slot as it was.
// - In 9:16, a slot dragged is written in the format's own slots, and the wide canvas keeps it.
// - New layout from this one copies it under a name asked for, and the state shown takes it.
// - axe-core finds nothing with the slots shown, and the keys sheet lists the gestures.
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

  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  const says = async (text) => {
    await page.waitForFunction((t) => document.querySelector("#status").textContent.includes(t), text, { timeout: 30000 }).catch(() => {});
    return (await status()).includes(text);
  };
  const slots = () => page.evaluate(() => window.scaena.canvas.slotting());
  const slot = async (name) => (await slots())?.slots.find((s) => s.name === name);
  /** Once slot `name` is written `col`, as the layout comes back from the engine. */
  const lands = async (name, col, row) => {
    await page
      .waitForFunction(
        ([n, c, r]) => {
          const s = window.scaena.canvas.slotting()?.slots.find((s) => s.name === n);
          return s && JSON.stringify(s.col) === JSON.stringify(c) && (r === undefined || JSON.stringify(s.row) === JSON.stringify(r));
        },
        [name, col, row],
        { timeout: 30000 },
      )
      .catch(() => {});
    return slot(name);
  };
  const onPage = ([x, y]) =>
    page.evaluate(
      ([x, y]) => {
        const r = document.querySelector("#overlay").getBoundingClientRect();
        const [w, h] = window.scaena.canvas.size();
        return [r.left + (x / w) * r.width, r.top + (y / h) * r.height];
      },
      [x, y],
    );
  const drag = async (from, to, mid) => {
    const [fx, fy] = await onPage(from);
    await page.mouse.move(fx, fy);
    await page.mouse.down();
    for (let k = 1; k <= 8; k++) {
      const [x, y] = await onPage([from[0] + ((to[0] - from[0]) * k) / 8, from[1] + ((to[1] - from[1]) * k) / 8]);
      await page.mouse.move(x, y);
    }
    if (mid) await mid();
    await page.mouse.up();
  };
  const box = (node) => page.evaluate((n) => window.scaena.canvas.boxes().find((b) => b.node === n)?.rect, node);

  // Layout: the slots of `figure`, each named.
  await page.click("#layout-edit");
  await page.waitForFunction(() => window.scaena.canvas.slotting()?.layout === "figure", null, { timeout: 30000 }).catch(() => {});
  const shown = await slots();
  check(shown?.layout === "figure" && shown.slots.map((s) => s.name).join() === "kicker,header,main,note", `Layout shows figure's slots: ${shown?.slots.map((s) => s.name)}`);
  check((await page.locator("#overlay svg .layout-slot").count()) === 4 && (await page.locator("#overlay svg .slot-name").count()) === 4, "each drawn and named");
  check((await page.locator("#layout-edit").getAttribute("aria-pressed")) === "true", "the button is pressed");
  await page.addScriptTag({ path: axe });
  const found = await page.evaluate(async () => {
    const result = await window.axe.run(document, { runOnly: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"] });
    return result.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
  });
  check(!found.length, `axe finds nothing with the slots shown${found.length ? `:\n    ${found.join("\n    ")}` : ""}`);

  // The note's slot, columns 1–8, dragged four columns right: columns 5–12.
  const note = await slot("note");
  const noteBox = await box("note");
  const column = note.rect[2] / 8;
  const [x, y, w, h] = note.rect;
  await drag([x + w / 2, y + h / 2], [x + w / 2 + column * 4, y + h / 2]);
  const moved = await lands("note", [5, 12]);
  check(JSON.stringify(moved?.col) === "[5,12]" && JSON.stringify(moved?.row) === "[11,12]", `the note's slot lands four columns right: ${JSON.stringify(moved?.col)}, ${JSON.stringify(moved?.row)}`);
  check(await says("every state that uses it shows it moved"), `the status says so: ${await status()}`);
  await page.waitForFunction((was) => window.scaena.canvas.boxes().find((b) => b.node === "note")?.rect[0] > was + 1, noteBox[0], { timeout: 30000 }).catch(() => {});
  check((await box("note"))[0] > noteBox[0] + column * 3, "the note in it moves with it");
  await page.selectOption("#state", "mix");
  await page.waitForFunction(() => window.scaena.canvas.boxed() === "mix", null, { timeout: 30000 }).catch(() => {});
  check((await box("note"))?.[0] > noteBox[0] + column * 3, "and in mix, which uses the layout too");
  await page.selectOption("#state", "revenue");
  await page.waitForFunction(() => window.scaena.canvas.boxed() === "revenue", null, { timeout: 30000 }).catch(() => {});
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+z");
  check(JSON.stringify((await lands("note", [1, 8]))?.col) === "[1,8]", "⌘Z takes it back");

  // The header's right handle, dragged two columns right: columns 1–10.
  const header = await slot("header");
  const [hx, hy, hw, hh] = header.rect;
  const handle = await page.evaluate(() => {
    const el = document.querySelector('#overlay svg [data-slot="header"][data-slot-edge="e"]');
    return el && [Number(el.getAttribute("x")) + Number(el.getAttribute("width")) / 2, Number(el.getAttribute("y")) + Number(el.getAttribute("height")) / 2];
  });
  check(handle !== null && Math.abs(handle[0] - (hx + hw)) < 1 && Math.abs(handle[1] - (hy + hh / 2)) < 1, "a handle on each side of a slot");
  await drag(handle, [handle[0] + column * 2, handle[1]]);
  check(JSON.stringify((await lands("header", [1, 10]))?.col) === "[1,10]", "the header's handle resizes it onto the grid: columns 1–10");
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+z");
  await lands("header", [1, 8]);

  // Escape through a drag: the slot as it was.
  const main = await slot("main");
  await drag([main.rect[0] + 40, main.rect[1] + 40], [main.rect[0] + 40 + column * 3, main.rect[1] + 40], () => page.keyboard.press("Escape"));
  await page.waitForTimeout(400);
  check(JSON.stringify((await slot("main"))?.col) === "[1,12]" && (await says("stays where it was")), `Escape leaves the slot as it was: ${await status()}`);

  // In 9:16 (the header's slot, near the top of the tall canvas): dragged, it is the format's
  // own; on the wide canvas it stays.
  await page.selectOption("#format", "9:16");
  await page.waitForFunction(() => window.scaena.canvas.slotting() && window.scaena.canvas.size()[1] > window.scaena.canvas.size()[0], null, { timeout: 30000 }).catch(() => {});
  await page.waitForTimeout(500);
  const tall = await slot("header");
  check(tall && !tall.own, `in 9:16 the header's slot is the layout's: ${JSON.stringify(tall)}`);
  const tallColumn = tall.rect[2] / 8;
  await drag([tall.rect[0] + tall.rect[2] / 2, tall.rect[1] + tall.rect[3] / 2], [tall.rect[0] + tall.rect[2] / 2 + tallColumn * 2, tall.rect[1] + tall.rect[3] / 2]);
  const own = await lands("header", [3, 10]);
  check(own?.own === true, `dragged, it is 9:16's own, columns 3–10: ${JSON.stringify(own)} · ${await status()}`);
  await page.selectOption("#format", "");
  await page.waitForFunction(() => window.scaena.canvas.size()[0] > window.scaena.canvas.size()[1], null, { timeout: 30000 }).catch(() => {});
  check(JSON.stringify((await lands("header", [1, 8]))?.col) === "[1,8]", "on the wide canvas, the header's slot stays at columns 1–8");
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+z");

  // A new layout from this one, by name: the state shown takes it.
  await page.evaluate(() => void window.scaena.canvas.newLayout());
  const field = await page.waitForSelector("input.note-words", { timeout: 10000 }).catch(() => null);
  check(field !== null, "New layout asks for its name");
  if (field) {
    await field.fill("figure-wide");
    await page.keyboard.press("Enter");
  }
  await page.waitForFunction(() => window.scaena.canvas.slotting()?.layout === "figure-wide", null, { timeout: 30000 }).catch(() => {});
  check((await slots())?.layout === "figure-wide", "the state shown takes the new layout, which the canvas shows");
  check((await page.evaluate(() => window.scaena.source())).includes("layout:figure-wide"), "the source says revenue uses it");

  // Escape leaves the layout.
  await page.locator("#overlay").focus();
  await page.keyboard.press("Escape");
  await page.waitForFunction(() => !window.scaena.canvas.slotting(), null, { timeout: 10000 }).catch(() => {});
  check(!(await slots()) && (await page.locator("#overlay svg .layout-slot").count()) === 0, "Escape leaves the layout");

  // The keys sheet.
  await page.keyboard.press("?");
  await page.waitForSelector("#keys[open]", { timeout: 10000 }).catch(() => {});
  const sheet = await page.evaluate(() => [...document.querySelectorAll("#keys tr")].map((tr) => tr.textContent));
  check(sheet.some((l) => l.includes("Drag a slot")), "the keys sheet lists dragging a slot");
} finally {
  await browser.close();
  await site.close();
}

if (failures.length) {
  console.log(`\n${failures.length} failed:\n  ${failures.join("\n  ")}`);
  process.exit(1);
}
console.log("\nall passed");

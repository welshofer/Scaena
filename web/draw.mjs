// PLAN 2.48 check: what the canvas draws, in headless Chromium (serve.mjs), the CPU painting, on
// the revenue example, in `revenue`.
//
//   node web/draw.mjs     (after `just web`; from the repository's root)
//
// - R arms the canvas to draw a rectangle. While the drag goes, the overlay shows where it lands;
//   let go, it is drawn over the grid's cells it covered, each edge on the nearest track's, in the
//   state shown, selected: one patch, one undo. The canvas draws one at a time.
// - L draws a line the way the drag went; A, nearly level from right to left, an arrow pointing
//   left.
// - With Shift, it is drawn off the grid, where the drag went: a `rect`.
// - T draws a text, which takes the caret with its words selected: what is typed takes their
//   place.
// - Armed, a click places it as Insert does; Escape, or the same key again, stops.
// Exits 1 on any failure.
import { launch, serve } from "./serve.mjs";

const server = await serve();
const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};

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
  const selected = () => page.evaluate(() => window.scaena.canvas.selected());
  const armed = () => page.evaluate(() => window.scaena.canvas.armed()?.key);
  const drawing = () => page.evaluate(() => document.querySelector("#overlay").classList.contains("drawing"));
  const box = (node) => page.evaluate((n) => window.scaena.canvas.boxes().find((b) => b.node === n)?.rect, node);
  /** The line of the source that declares `node`. */
  const declared = async (node) => (await source()).split("\n").find((l) => l.trimStart().startsWith(`${node} `)) ?? "";
  /** Once the source holds `text`, and the canvas has caught up. */
  const reads = async (text) => {
    await page
      .waitForFunction((t) => window.scaena.source().includes(t) && !window.scaena.canvas.busy(), text, { timeout: 30000, polling: 50 })
      .catch(() => {});
    await page.waitForTimeout(300);
    return (await source()).includes(text);
  };
  const back = async (to) => {
    await page.waitForFunction((s) => window.scaena.source() === s, to, { timeout: 30000, polling: 50 }).catch(() => {});
    await page.waitForTimeout(300);
    return (await source()) === to;
  };
  const says = async (text) => {
    await page.waitForFunction((t) => document.querySelector("#status").textContent.includes(t), text, { timeout: 30000 }).catch(() => {});
    return (await status()).includes(text);
  };
  /** The source as it was, as one edit of it; the boxes those of `revenue` again. What is drawn
   * leaves at the next slide (PLAN 3.24), so the edit reaches `close` too: the cursor goes back to
   * `revenue`, which the preview follows. */
  const restore = async (to) => {
    if ((await source()) !== to) await page.evaluate((s) => window.scaena.type(s), to);
    await back(to);
    await page.evaluate((offset) => window.scaena.cursor(offset), to.indexOf("state revenue") + "state ".length);
    await page.waitForFunction(() => window.scaena.canvas.boxed() === "revenue" && !window.scaena.canvas.busy(), null, { timeout: 30000 });
  };
  /** Client px of `at`, canvas units. */
  const client = ([x, y]) =>
    page.evaluate(
      ([x, y]) => {
        const r = document.querySelector("#overlay").getBoundingClientRect();
        const [w, h] = window.scaena.canvas.size();
        return [r.left + (x / w) * r.width, r.top + (y / h) * r.height];
      },
      [x, y],
    );
  /** Press `key` on the canvas. */
  const key = async (k) => {
    await page.locator("#overlay").focus();
    await page.keyboard.press(k);
  };
  /** What the canvas draws once a key armed it: at once, so a drag that follows draws. */
  const arms = async (k) => {
    await key(k);
    return (await armed()) === k && (await drawing());
  };
  /** A drag on the canvas from `from` to `to`, canvas units; with Shift held, `shift`. Before it
   * lets go, `held` is asked what the overlay shows. */
  const drag = async (from, to, shift = false, held = async () => {}) => {
    const [[fx, fy], [tx, ty]] = [await client(from), await client(to)];
    if (shift) await page.keyboard.down("Shift");
    await page.mouse.move(fx, fy);
    await page.mouse.down();
    await page.mouse.move(tx, ty, { steps: 8 });
    await held();
    await page.mouse.up();
    if (shift) await page.keyboard.up("Shift");
  };

  // In `revenue`, the slide the example's chart is on.
  const at = (await source()).indexOf("state revenue") + "state ".length;
  await page.evaluate((offset) => window.scaena.cursor(offset), at);
  await page.waitForFunction(() => window.scaena.shown() === 1 && window.scaena.canvas.boxed() === "revenue", null, { timeout: 30000 });
  const original = await source();
  const grid = await page.evaluate(() => window.scaena.canvas.targets("title"));
  const [cols, rows] = [grid.columns, grid.rows];
  const step = cols[1][0] - cols[0][0];
  const on = (tracks, at, side) => tracks.some((t) => Math.abs(t[side] - at) < 0.01);
  const near = (a, b) => Math.abs(a - b) <= step / 2 + 0.01;

  // R arms the canvas; a drag draws a rectangle over the cells it covers.
  check(await arms("r"), "R arms the canvas to draw a rectangle, at once");
  check(await says("drawing Shape · rect"), `and says so: ${await status()}`);
  let landing = 0;
  await drag([300, 200], [1100, 700], false, async () => {
    await page.waitForFunction(() => document.querySelectorAll("#overlay svg rect.landing").length > 0, null, { timeout: 30000 }).catch(() => {});
    landing = await page.evaluate(() => document.querySelectorAll("#overlay svg rect.landing").length);
  });
  check(landing === 1, "while the drag goes, the overlay shows where it lands");
  check(await reads("rect shape"), `let go, it is drawn: ${await declared("rect")}`);
  check(await says("rect drawn as rect, in revenue"), `the status says what was drawn: ${await status()}`);
  check((await selected()) === "rect", "selected");
  const r = await box("rect");
  check(Boolean(r) && on(cols, r[0], 0) && on(cols, r[0] + r[2], 1) && on(rows, r[1], 0) && on(rows, r[1] + r[3], 1), `over the grid's cells: ${r}`);
  check(Boolean(r) && near(r[0], 300) && near(r[0] + r[2], 1100) && near(r[1], 200) && near(r[1] + r[3], 700), `the nearest the drag's edges: ${r}`);
  check((await armed()) === undefined && !(await drawing()), "and the canvas draws one at a time");
  const scn = await source();
  check(scn.indexOf("rect shape") > scn.indexOf("state revenue") && scn.indexOf("rect shape") < scn.indexOf("state mix"), "it enters in `revenue`");
  await key("Control+z");
  check(await back(original), "one undo takes it back");

  // L draws a line the way the drag went: up and to the right, from the bottom left corner.
  await restore(original);
  check(await arms("l"), "L arms it to draw a line");
  await drag([400, 900], [1200, 300]);
  check(await reads("line shape"), "L draws a line");
  check(/points:\[\[0(\.0)?, 1(\.0)?\], \[1(\.0)?, 0(\.0)?\]\]/.test(await declared("line")), `up and to the right, the way the drag went: ${await declared("line")}`);

  // A, nearly level, from right to left: an arrow pointing left.
  await restore(original);
  check(await arms("a"), "A, an arrow");
  await drag([1200, 500], [400, 480]);
  check(await reads("arrow shape"), "A draws an arrow");
  check(/points:\[\[1(\.0)?, 0\.5\], \[0(\.0)?, 0\.5\]\]/.test(await declared("arrow")), `level, pointing left: ${await declared("arrow")}`);

  // With Shift, off the grid, where the drag went.
  await restore(original);
  check(await arms("o"), "O, an ellipse");
  await drag([300, 200], [700, 520], true);
  check(await reads("ellipse shape"), "O draws an ellipse");
  check(/at:rect\(300(\.0)?, 200(\.0)?, 400(\.0)?, 320(\.0)?\)/.test(await declared("ellipse")), `with Shift, where the drag went: ${await declared("ellipse")}`);
  check(JSON.stringify(await box("ellipse")) === JSON.stringify([300, 200, 400, 320]), `and it stands there: ${await box("ellipse")}`);

  // T draws a text that takes the caret, its words selected: what is typed takes their place.
  await restore(original);
  check(await arms("t"), "T arms the canvas to draw a text");
  await drag([300, 760], [1100, 900]);
  const text = await page
    .waitForFunction(() => window.scaena.canvas.typed()?.value.length > 0 && window.scaena.canvas.typed(), null, { timeout: 30000 })
    .then((h) => h.jsonValue(), () => undefined);
  check(Boolean(text) && text.from === 0 && text.to === text.value.length, `a text drawn takes the caret, its words selected: ${JSON.stringify(text)}`);
  const id = await page.evaluate(() => window.scaena.canvas.typing());
  check(Boolean(id) && (await declared(id)).includes(" text "), `it is a text: ${id}`);
  await page.keyboard.type("Drawn by hand");
  check(await reads('"Drawn by hand"'), `what is typed takes their place: ${await declared(id)}`);
  await page.keyboard.press("Escape");

  // Armed, a click places it as Insert does: about the point pressed.
  await restore(original);
  check(await arms("r"), "R again");
  const [cx, cy] = await client([960, 540]);
  await page.mouse.click(cx, cy);
  check(await reads("rect shape"), "a click, armed, places it");
  check(await says("rect inserted as rect, in revenue"), `as Insert does: ${await status()}`);
  const placed = await box("rect");
  check(Boolean(placed) && placed[0] <= 960 && 960 <= placed[0] + placed[2] && placed[1] <= 540 && 540 <= placed[1] + placed[3], `about the point pressed: ${placed}`);

  // Escape stops drawing, and so does the same key again; nothing is drawn.
  await restore(original);
  await key("o");
  await key("Escape");
  check((await armed()) === undefined && !(await drawing()), "Escape stops drawing");
  check(await says("not drawing"), `and says so: ${await status()}`);
  await key("l");
  await key("l");
  check((await armed()) === undefined, "and so does the same key again");
  check((await source()) === original, "nothing is drawn");
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

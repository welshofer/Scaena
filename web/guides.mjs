// PLAN 2.57 check: guides on the editor's canvas, in headless Chromium (serve.mjs), the CPU
// painting, on the torture deck's `containers` case, whose theme's grid is 12 × 8 inside a 96 cu
// margin with a baseline every 8 cu.
//
//   node web/guides.mjs     (after `just web`; from the repository's root)
//
// - The theme's grid is drawn over the canvas on request: ⌘' (Ctrl+'), the Grid button, and the
//   command each draw it or take it away. Drawn, it is the engine's: each column and row, the
//   margin around them, and every line of the baseline grid. In another format, that format's.
// - As a node is dragged on the grid, a guide shows wherever one of its edges, or its middle,
//   meets another node's or the canvas's: the card's top meets the board's beside it.
// - Off the grid (Shift), a node dragged within a few pixels of another's edge goes onto it, and
//   the patch puts it there; with Ctrl (⌘) held, or farther off, it stays where the drag left it. A handle dragged with
//   Shift within a few pixels of the canvas's edge takes the box's edge onto it.
// - Several dragged together off the grid go as one box.
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
  await page.goto(`${server.origin}/web/dist/editor.html?painter=cpu&bundle=/tests/fixtures/torture.scaena/deck.json`);
  await page.waitForFunction(() => window.scaena?.last(), null, { timeout: 120000 });

  const source = () => page.evaluate(() => window.scaena.source());
  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  const trips = () => page.evaluate(() => window.scaena.trips().length);
  /** Once lint has answered for the source the editor holds after `before` trips. */
  const settled = (before) =>
    page.waitForFunction((n) => window.scaena.trips().length > n && !window.scaena.canvas.busy(), before, { timeout: 60000, polling: 50 });
  const box = (node) => page.evaluate((n) => window.scaena.canvas.boxes().find((b) => b.node === n)?.rect, node);
  const center = ([x, y, w, h]) => [x + w / 2, y + h / 2];
  /** A point in canvas units, on the page. */
  const client = ([x, y]) =>
    page.evaluate(
      ([x, y]) => {
        const r = document.querySelector("#overlay").getBoundingClientRect();
        const [w, h] = window.scaena.canvas.size();
        return [r.left + (x / w) * r.width, r.top + (y / h) * r.height];
      },
      [x, y],
    );
  /** Canvas units in a CSS pixel, at the zoom shown. */
  const unit = () => page.evaluate(() => window.scaena.canvas.size()[0] / document.querySelector("#overlay").getBoundingClientRect().width);
  const drawn = (cls) => page.evaluate((c) => document.querySelectorAll(`#overlay .${c}`).length, cls);
  const pressed = () => page.evaluate(() => document.querySelector("#grid").getAttribute("aria-pressed"));
  /** Show the state declared as `state` in the source, and what stands where in it. */
  const into = async (state) => {
    await page.evaluate((s) => window.scaena.cursor(window.scaena.source().indexOf(`state ${s}`) + "state ".length), state);
    await page.waitForFunction((s) => window.scaena.canvas.boxed() === s && window.scaena.canvas.boxes().length > 0, state, { timeout: 30000 });
  };
  /** Press at `from`, move past the slop and on to `to` (canvas units) with `keys` held, and
   * wait until the status says where it lands; the status then, and the guides shown. Let go
   * unless `hold`. */
  const drag = async (from, to, keys = [], { hold = false, says = "→" } = {}) => {
    const [fx, fy] = await client(from);
    const [tx, ty] = await client(to);
    await page.mouse.move(fx, fy);
    await page.mouse.down();
    for (const key of keys) await page.keyboard.down(key);
    await page.mouse.move(fx + 8, fy + 8, { steps: 2 });
    await page.mouse.move(tx, ty, { steps: 8 });
    await page.waitForFunction((s) => document.querySelector("#status").textContent.includes(s), says, { timeout: 30000 }).catch(() => {});
    // The last move's request answered.
    await page.waitForTimeout(250);
    const said = await status();
    const guides = await page.evaluate(() => window.scaena.canvas.guides());
    const lines = await drawn("guide");
    if (!hold) {
      await page.mouse.up();
      for (const key of keys) await page.keyboard.up(key);
    }
    return { said, guides, lines };
  };
  const undo = async () => {
    const n = await trips();
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+z");
    await settled(n);
  };

  await into("containers");
  const original = await source();

  // The grid: off until asked for.
  check((await drawn("grid-track")) === 0 && (await pressed()) === "false", "the grid is not drawn until asked for");
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+'");
  await page.waitForFunction(() => document.querySelectorAll("#overlay .grid-track").length > 0, null, { timeout: 30000 }).catch(() => {});
  const grid = await page.evaluate(() => window.scaena.canvas.grid());
  check(
    grid?.columns.length === 12 && grid.rows.length === 8 && grid.columns[0][0] === 96 && grid.columns[11][1] === 1824,
    `⌘' draws the theme's grid, the engine's: ${JSON.stringify(grid && { columns: grid.columns.length, rows: grid.rows.length, first: grid.columns[0], last: grid.columns[11] })}`,
  );
  check((await drawn("grid-track")) === 20 && (await drawn("grid-margin")) === 1, `each column and row, and the margin: ${await drawn("grid-track")} tracks`);
  check(
    (await drawn("baseline")) === 112 && grid.baselines[0] === 96 && grid.baselines.at(-1) === 984,
    `and every line of the baseline grid, 8 cu apart from the top margin: ${await drawn("baseline")}`,
  );
  check((await pressed()) === "true", "the Grid button says it is drawn");
  check((await status()).includes("12 columns and 8 rows, and the baseline grid"), `the status says what: ${await status()}`);

  // Another format: that format's grid, on its canvas.
  await page.locator("#format").selectOption("9:16");
  await page.waitForFunction(() => window.scaena.canvas.grid()?.canvas[0] === 1080 && window.scaena.canvas.size()[0] === 1080, null, { timeout: 60000 }).catch(() => {});
  const tall = await page.evaluate(() => window.scaena.canvas.grid());
  check(
    tall?.canvas[1] === 1920 && Math.abs(tall.columns[0][0] - 64) < 0.01 && (await drawn("grid-track")) === 20,
    `in 9:16, the format's own grid: ${JSON.stringify(tall && { canvas: tall.canvas, left: tall.columns[0][0] })}`,
  );
  await page.locator("#format").selectOption("");
  await page.waitForFunction(() => window.scaena.canvas.grid()?.canvas[0] === 1920 && window.scaena.canvas.size()[0] === 1920, null, { timeout: 60000 }).catch(() => {});
  await into("containers");

  // The button takes it away, and the command draws it again.
  await page.locator("#grid").click();
  await page.waitForFunction(() => !document.querySelector("#overlay .grid-track"), null, { timeout: 10000 }).catch(() => {});
  check((await drawn("grid-track")) === 0 && (await pressed()) === "false", "the Grid button takes it away");
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+k");
  await page.waitForFunction(() => document.querySelector("#palette").open, null, { timeout: 10000 });
  await page.keyboard.type("show the grid");
  const offered = await page.evaluate(() => window.scaena.commands.shown());
  const key = await page.evaluate(() => document.querySelector("#palette [role=option] kbd")?.textContent ?? "");
  check(offered[0] === "Show the grid" && key.endsWith("'"), `the palette offers it, with its key: ${offered[0]}, ${key}`);
  await page.keyboard.press("Enter");
  await page.waitForFunction(() => document.querySelectorAll("#overlay .grid-track").length === 20, null, { timeout: 10000 }).catch(() => {});
  check((await drawn("grid-track")) === 20, "and draws it");
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+'");
  await page.waitForFunction(() => !document.querySelector("#overlay .grid-track"), null, { timeout: 10000 }).catch(() => {});
  check((await drawn("grid-track")) === 0, "⌘' takes it away again");

  // On the grid: the card, a frame in columns 9–12 beside the board, dragged by its panel. Its
  // top meets the board's, where both stand on the sixth row.
  const card = await box("card");
  const board = await box("board");
  const photo = await box("board-photo");
  const panel = [card[0] + card[2] - 12, card[1] + card[3] - 12];
  await page.evaluate(() => window.scaena.canvas.select("card"));
  await page.waitForFunction(() => window.scaena.canvas.selected() === "card", null, { timeout: 10000 });
  // Far enough past the slop that a late answer from the engine still finds a drag; not so far
  // that it reaches another track.
  const onGrid = await drag(panel, [panel[0] + 24, panel[1] + 16], [], { hold: true, says: "card →" });
  const top = onGrid.guides.find((g) => g[1] === g[3] && Math.abs(g[1] - board[1]) < 0.5);
  check(Boolean(top) && top[0] <= board[0] && top[2] >= card[0] + card[2], `on the grid, a guide where its top meets the board's: ${JSON.stringify(onGrid.guides)} (${onGrid.said})`);
  check(onGrid.lines === onGrid.guides.length && onGrid.lines > 0, `drawn over the canvas: ${onGrid.lines} lines`);
  await page.keyboard.press("Escape");
  await page.mouse.up();
  check((await source()) === original, "Escape leaves it where it was");

  // Off the grid: its left edge dragged to 3 units right of the photo's, inside the reach of a
  // few pixels: onto the photo's edge.
  const u = await unit();
  check(3 < 6 * u, `3 canvas units are within the reach of 6 pixels: ${(6 * u).toFixed(1)} units`);
  let n = await trips();
  const by = [photo[0] + 3 - card[0], -37];
  const near = await drag(panel, [panel[0] + by[0], panel[1] + by[1]], ["Shift"], { says: "rect" });
  await settled(n);
  const left = Math.round(photo[0]);
  check(near.said.includes(`rect ${left}, `), `with Shift, its edge goes onto the photo's: ${near.said} (the photo at ${photo[0]})`);
  const down = near.guides.find((g) => g[0] === g[2] && Math.abs(g[0] - photo[0]) <= 0.5);
  check(Boolean(down), `a guide down the edges that meet: ${JSON.stringify(near.guides)}`);
  check((await box("card"))?.[0] === left, `the patch puts it there: ${JSON.stringify(await box("card"))}`);
  await undo();
  check((await source()) === original, "one undo takes it back");

  // With Ctrl (⌘) held too, the same drag goes where the pointer says, onto no guide.
  n = await trips();
  const loose = await drag(panel, [panel[0] + by[0], panel[1] + by[1]], ["Shift", "Control"], { says: "rect" });
  await settled(n);
  check(loose.said.includes(`rect ${Math.round(photo[0] + 3)}, `), `with Ctrl held, it goes where the pointer says: ${loose.said}`);
  await undo();

  // Farther off than the reach from every edge and middle across: where the drag left it.
  const clear = await page.evaluate(
    ([card, reach]) => {
      const boxes = window.scaena.canvas.boxes();
      const parent = (id) => boxes.find((b) => b.node === id)?.parent;
      const moves = (id) => {
        for (let at = id, k = 0; at !== undefined && k <= boxes.length; at = parent(at), k++) if (at === "card") return true;
        return false;
      };
      const [w] = window.scaena.canvas.size();
      const stops = [0, w / 2, w];
      for (const b of boxes) if (b.draws && !moves(b.node)) stops.push(b.rect[0], b.rect[0] + b.rect[2] / 2, b.rect[0] + b.rect[2]);
      for (let x = 200; x < w - card[2]; x++) {
        const mine = [x, x + card[2] / 2, x + card[2]];
        if (mine.every((m) => stops.every((s) => Math.abs(m - s) > reach + 2))) return x;
      }
      return undefined;
    },
    [card, 6 * u],
  );
  n = await trips();
  const away = await drag(panel, [panel[0] + clear - card[0], panel[1] - 37], ["Shift"], { says: "rect" });
  await settled(n);
  check(away.said.includes(`rect ${clear}, `), `farther off, it stays where the drag left it: ${away.said} (at ${clear})`);
  await undo();

  // A handle dragged with Shift: the right edge, 4 units short of the canvas's, onto it. The undo
  // shown on the canvas first, so the handle stands where the card is again, not where the drag
  // before left it: a press there would move the card.
  await page
    .waitForFunction((c) => window.scaena.canvas.boxes().find((b) => b.node === "card")?.rect.every((v, i) => Math.abs(v - c[i]) < 0.5), card, { timeout: 30000 })
    .catch(() => {});
  await page.evaluate(() => window.scaena.canvas.select("card"));
  await page.waitForFunction(() => document.querySelectorAll("#overlay rect.handle").length === 8, null, { timeout: 30000 }).catch(() => {});
  n = await trips();
  const edge = [card[0] + card[2], card[1] + card[3] / 2];
  const wide = await drag(edge, [1920 - 4, edge[1]], ["Shift"], { says: "rect" });
  await settled(n);
  check(wide.said.includes(`rect ${card[0]}, ${card[1]}, ${1920 - card[0]} × `), `a handle with Shift takes the edge onto the canvas's: ${wide.said}`);
  await undo();
  check((await source()) === original, "and one undo takes that back");

  // Several dragged together off the grid go as one box: the board and the card, 3 units right,
  // back onto the edges where they stand.
  await page.evaluate(() => window.scaena.canvas.selectAll(["board", "card"]));
  await page.waitForFunction(() => window.scaena.canvas.chosen().length === 2, null, { timeout: 10000 });
  const both = await drag(panel, [panel[0] + 3, panel[1]], ["Shift"], { hold: true, says: "move together" });
  check(both.guides.some((g) => g[0] === g[2]), `the box around them meets the edges it left: ${JSON.stringify(both.guides)}`);
  await page.keyboard.press("Escape");
  await page.mouse.up();
  await page.keyboard.up("Shift");
  check((await source()) === original, "and Escape leaves them where they were");
} catch (e) {
  failures.push(String(e));
} finally {
  await browser.close();
  server.close();
}

if (failures.length) {
  console.error(`\n${failures.length} failed:\n${failures.map((f) => `  ${f}`).join("\n")}`);
  process.exit(1);
}
console.log("\nall guides checks pass");

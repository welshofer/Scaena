// PLAN 2.109 check: connectors, on the editor's canvas, in headless Chromium (serve.mjs), the CPU
// painting, as in web/canvas.mjs.
//
//   node web/connect.mjs     (after `just web`; from the repository's root)
//
// On the revenue example, in `revenue`:
// - The title selected, then the note under the chart with Shift; the palette's Connect with an
//   arrow joins the title to the note: `title-to-note`, an arrow whose `connect` names them and
//   nothing else places, selected, one step to undo.
// - It runs between them, over the chart, and a click on its line selects it; a click beside the
//   line picks the chart behind it.
// - A drag that starts on it moves nothing, and says to move either end.
// - A drag of the title off the grid moves the title, and the arrow runs from where it went.
// - Undo takes back the drag, then the arrow.
// Exits 1 on any failure.
import { launch, serve } from "./serve.mjs";

const server = await serve();
const url = (bundle) => `${server.origin}/web/dist/editor.html?painter=cpu&bundle=${bundle}`;
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
  await page.goto(url("/docs/examples/revenue.deck.json"));
  await page.waitForFunction(() => window.scaena?.last() || document.querySelector("#status")?.textContent.startsWith("Error"), null, {
    timeout: 120000,
  });

  const source = () => page.evaluate(() => window.scaena.source());
  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  const selected = () => page.evaluate(() => window.scaena.canvas.selected());
  /** Once the canvas selects `id`: whether it does within the wait. */
  const selects = (id) =>
    page
      .waitForFunction((n) => window.scaena.canvas.selected() === n, id, { timeout: 10000 })
      .then(() => true, () => false);
  const boxOf = (node) => page.evaluate((n) => window.scaena.canvas.boxes().find((b) => b.node === n), node);
  /** Once the source holds `text`, and the canvas has caught up. */
  const reads = async (text) => {
    await page
      .waitForFunction((t) => window.scaena.source().includes(t) && !window.scaena.canvas.busy(), text, { timeout: 30000, polling: 50 })
      .catch(() => {});
    await page.waitForTimeout(300);
    return (await source()).includes(text);
  };
  const back = async (to) => {
    await page.waitForFunction((s) => window.scaena.source() === s && !window.scaena.canvas.busy(), to, { timeout: 30000, polling: 50 }).catch(() => {});
    await page.waitForTimeout(300);
    return (await source()) === to;
  };
  /** A canvas point as the page's. */
  const client = ([x, y]) =>
    page.evaluate(
      ([x, y]) => {
        const r = document.querySelector("#overlay").getBoundingClientRect();
        const [w, h] = window.scaena.canvas.size();
        return [r.left + (x / w) * r.width, r.top + (y / h) * r.height];
      },
      [x, y],
    );
  const middle = ([x, y, w, h]) => [x + w / 2, y + h / 2];
  /** The declaration of `node` in the source, with the lines its properties run on to, as one. */
  const declared = (scn, node) => {
    const lines = scn.split("\n");
    const line = lines.findIndex((l) => l.trimStart().startsWith(`${node} `) || l.startsWith(`node ${node} `));
    if (line < 0) return "";
    const indent = (l) => l.length - l.trimStart().length;
    const rest = lines.slice(line + 1);
    const end = rest.findIndex((l) => indent(l) <= indent(lines[line]));
    return [lines[line], ...rest.slice(0, end < 0 ? rest.length : end)].map((l) => l.trim()).join(" ");
  };
  /** A drag on the canvas from `from` to `to`, canvas units, in steps; with `off`, Shift held, off
   * the grid, so that it lands where it is let go. */
  const drag = async (from, to, off = false) => {
    const [fx, fy] = await client(from);
    const [tx, ty] = await client(to);
    await page.mouse.move(fx, fy);
    if (off) await page.keyboard.down("Shift");
    await page.mouse.down();
    for (let i = 1; i <= 8; i++) await page.mouse.move(fx + ((tx - fx) * i) / 8, fy + ((ty - fy) * i) / 8);
    await page.mouse.up();
    if (off) await page.keyboard.up("Shift");
  };

  // In `revenue`.
  await page.evaluate(() => window.scaena.cursor(window.scaena.source().indexOf("state revenue") + "state ".length));
  await page.waitForFunction(() => window.scaena.shown() === 1 && window.scaena.canvas.boxed() === "revenue", null, { timeout: 30000 });
  const original = await source();
  const [title, note] = [(await boxOf("title")).rect, (await boxOf("note")).rect];

  // The title, then the note with Shift.
  await page.mouse.click(...(await client(middle(title))));
  check(await selects("title"), "a click selects the title");
  await page.keyboard.down("Shift");
  await page.mouse.click(...(await client(middle(note))));
  await page.keyboard.up("Shift");
  await page.waitForFunction(() => window.scaena.canvas.chosen().length === 2, null, { timeout: 10000 }).catch(() => {});
  const chosen = await page.evaluate(() => window.scaena.canvas.chosen());
  check(chosen.join(" ") === "title note", `Shift and a click select the note beside it: ${chosen}`);

  // The palette's Connect with an arrow.
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+k");
  await page.waitForFunction(() => document.querySelector("#palette").open, null, { timeout: 10000 });
  await page.keyboard.type("connect with an arrow");
  const listed = await page.evaluate(() => window.scaena.commands.shown());
  check(listed[0] === "Connect with an arrow", `the palette offers it first: ${listed.slice(0, 3)}`);
  await page.keyboard.press("Enter");
  check(await reads("title-to-note"), `the title and the note are joined: ${await status()}`);
  const joined = await source();
  const arrow = declared(joined, "title-to-note");
  check(/shape kind:arrow/.test(arrow) && /connect/.test(arrow) && arrow.includes("title") && arrow.includes("note"), `an arrow whose connect names them: ${arrow}`);
  check(!/rect\(|col\(|in\(|points/.test(arrow), `and nothing else places it: ${arrow}`);
  check(await selects("title-to-note"), `the arrow is selected: ${await selected()}`);

  // It runs between them, over the chart, and is picked by its line.
  const box = await boxOf("title-to-note");
  const route = box?.route ?? [];
  check(route.length === 2 && box.joins?.join(" ") === "title note", `its box names what it joins and its line: ${JSON.stringify(box)}`);
  check(route[0][1] > title[1] + title[3] && route[1][1] < note[1], `from under the title to over the note: ${JSON.stringify(route)}`);
  const along = [(route[0][0] + route[1][0]) / 2, (route[0][1] + route[1][1]) / 2];
  await page.keyboard.press("Escape");
  await page.mouse.click(...(await client([along[0] + 60, along[1]])));
  check(await selects("rev"), `a click beside its line picks the chart behind it: ${await selected()}`);
  await page.mouse.click(...(await client(along)));
  check(await selects("title-to-note"), `a click on its line selects it: ${await selected()}`);

  // A drag that starts on it moves nothing.
  await drag(along, [along[0] + 200, along[1] + 60]);
  await page.waitForTimeout(800);
  check((await source()) === joined, "a drag of the arrow moves nothing");
  check(/move either/.test(await status()), `and says to move either end: ${await status()}`);

  // A drag of the title: the arrow runs from where it went.
  await page.mouse.click(...(await client(middle(title))));
  check(await selects("title"), `a click selects the title: ${await selected()}`);
  await drag(middle(title), [middle(title)[0] + 300, middle(title)[1]], true);
  await page.waitForFunction((s) => window.scaena.source() !== s && !window.scaena.canvas.busy(), joined, { timeout: 30000, polling: 50 }).catch(() => {});
  await page.waitForTimeout(500);
  const moved = await source();
  check(moved !== joined && declared(moved, "title-to-note") === arrow, "the drag moves the title, and the arrow's declaration stays as it was");
  const title2 = (await boxOf("title"))?.rect;
  const route2 = (await boxOf("title-to-note"))?.route ?? [];
  check(Boolean(title2) && title2[0] > title[0], `the title went right: ${title2} from ${title}`);
  check(route2.length === 2 && route2[0][0] > route[0][0], `and the arrow runs from where it went: ${JSON.stringify(route2)} from ${JSON.stringify(route)}`);

  // Each step undone.
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+z");
  check(await back(joined), "undo takes the drag back");
  await page.keyboard.press("Control+z");
  check(await back(original), "then the arrow");
  check(await page.evaluate(() => window.scaena.last().valid), "the source still compiles and validates");
} finally {
  await browser.close();
  await server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "two objects are joined by a connector that follows them");
process.exit(failures.length ? 1 : 0);

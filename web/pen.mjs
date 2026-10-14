// PLAN 2.114 check: the pen, in headless Chromium (serve.mjs), the CPU painting, on the revenue
// example, in `revenue`.
//
//   node web/pen.mjs     (after `just web`; from the repository's root)
//
// - P takes up the pen: the status says so, and the canvas draws with it.
// - A drag draws a stroke, its ink shown as it goes; let go, it is a drawing where it was drawn: a
//   path shape on its own view in the theme's accent, selected, one patch.
// - A second stroke joins the drawing: one more curve of its path, its box grown to take it in.
// - One undo takes the second stroke away, and another the drawing.
// - Escape puts the pen down.
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
  const pen = () => page.evaluate(() => window.scaena.canvas.pen());
  const drawing = () => page.evaluate(() => document.querySelector("#overlay").classList.contains("drawing"));
  const box = (node) => page.evaluate((n) => window.scaena.canvas.boxes().find((b) => b.node === n)?.rect, node);
  /** The lines of the source that declare `node`, and those under it, as one. */
  const declared = async (node) => {
    const lines = (await source()).split("\n");
    const first = lines.findIndex((l) => l.trimStart().startsWith(`${node} `));
    if (first < 0) return "";
    const deeper = (l) => l.search(/\S/) > lines[first].search(/\S/);
    let last = first + 1;
    while (last < lines.length && deeper(lines[last])) last++;
    return lines.slice(first, last).join(" ");
  };
  /** How many strokes the drawing's path holds: one `M` each. */
  const strokes = async () => ((await declared("drawing")).match(/path:"([^"]*)"/)?.[1] ?? "").split("M").length - 1;
  const settled = (pass) =>
    page
      .waitForFunction(pass, null, { timeout: 30000, polling: 50 })
      .then(() => page.waitForFunction(() => !window.scaena.canvas.busy(), null, { timeout: 30000 }))
      .then(
        () => true,
        () => false,
      );
  const says = async (text) => {
    await page.waitForFunction((t) => document.querySelector("#status").textContent.includes(t), text, { timeout: 30000 }).catch(() => {});
    return (await status()).includes(text);
  };
  const back = async (to) => {
    await page.waitForFunction((s) => window.scaena.source() === s, to, { timeout: 30000, polling: 50 }).catch(() => {});
    await page.waitForTimeout(300);
    return (await source()) === to;
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
  const key = async (k) => {
    await page.locator("#overlay").focus();
    await page.keyboard.press(k);
  };
  /** A stroke through `points`, canvas units; before it lets go, `held` is asked what shows. */
  const stroke = async (points, held = async () => {}) => {
    const at = [];
    for (const p of points) at.push(await client(p));
    await page.mouse.move(...at[0]);
    await page.mouse.down();
    for (const p of at.slice(1)) await page.mouse.move(...p);
    await held();
    await page.mouse.up();
  };

  // In `revenue`, the slide the example's chart is on.
  const at = (await source()).indexOf("state revenue") + "state ".length;
  await page.evaluate((offset) => window.scaena.cursor(offset), at);
  await page.waitForFunction(() => window.scaena.shown() === 1 && window.scaena.canvas.boxed() === "revenue", null, { timeout: 30000 });
  const original = await source();

  // P takes up the pen.
  await key("p");
  check(await settled(() => window.scaena.canvas.pen() !== undefined), "P takes up the pen");
  check(await drawing(), "and the canvas draws with it");
  check(await says("Drawing with the pen"), `the status says so: ${await status()}`);

  // A stroke: its ink shown as it goes, then a drawing where it was drawn.
  const wave = Array.from({ length: 31 }, (_, i) => [300 + i * 20, 750 + 50 * Math.sin(i / 3)]);
  let ink = 0;
  await stroke(wave, async () => {
    ink = await page.evaluate(() => document.querySelectorAll("#overlay svg polyline.ink").length);
  });
  check(ink === 1, "the stroke's ink shows as it goes");
  check(await settled(() => window.scaena.source().includes("drawing shape")), `let go, it is a drawing: ${await declared("drawing")}`);
  check(/kind:path\b/.test(await declared("drawing")) && /view:\[/.test(await declared("drawing")), "a path on its own view");
  check(/accent/.test(await declared("drawing")), "in the theme's accent");
  check(await says("A drawing drawn on this slide"), `the status says what was drawn: ${await status()}`);
  check((await selected()) === "drawing", "selected");
  // The inspector heads it a Drawing, no icon for the icon picker to replace.
  await page.waitForFunction(() => document.querySelector("#look h2")?.textContent === "Drawing", null, { timeout: 30000 }).catch(() => {});
  const heading = await page.evaluate(() => document.querySelector("#look h2")?.textContent);
  check(heading === "Drawing", `the inspector heads it a Drawing: ${heading}`);
  const first = await box("drawing");
  check(Boolean(first) && first[0] <= 300 && first[0] + first[2] >= 900 && first[1] <= 700 && first[1] + first[3] >= 800, `where it was drawn: ${first}`);
  check((await strokes()) === 1, "one stroke");
  const once = await source();
  check(await settled(() => window.scaena.canvas.pen()?.onto === "drawing"), "the pen stays up, its next stroke the drawing's");

  // A second stroke joins the drawing.
  await stroke(Array.from({ length: 11 }, (_, i) => [350 + i * 50, 900]));
  check(await settled(() => window.scaena.source() !== null && document.querySelector("#status").textContent.includes("A stroke added")), `a second stroke joins it: ${await status()}`);
  check((await strokes()) === 2, `its path holds both: ${await strokes()}`);
  const both = await box("drawing");
  check(Boolean(both) && both[0] === first[0] && both[1] === first[1] && both[1] + both[3] >= 900, `its box grown to take it in, the first where it was: ${both}`);

  // One undo each.
  await key("Control+z");
  check(await back(once), "one undo takes the second stroke away");
  await key("Control+z");
  check(await back(original), "and another the drawing");
  check(await page.evaluate(() => window.scaena.last().valid), "and the deck validates");

  // Escape puts the pen down.
  await key("Escape");
  check(await settled(() => window.scaena.canvas.pen() === undefined), "Escape puts the pen down");
  check(!(await drawing()) && (await says("The pen is put down")), `and says so: ${await status()}`);
} catch (e) {
  failures.push(`error: ${e.message}`);
} finally {
  await browser.close();
  server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "the pen draws a drawing of strokes, each one step to undo");
process.exit(failures.length ? 1 : 0);

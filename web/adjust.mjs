// PLAN 2.117 check: an image's adjustments from the inspector, in headless Chromium (serve.mjs),
// the CPU painting, on the torture deck's `adjust` state.
//
//   node web/adjust.mjs     (after `just web`; from the repository's root)
//
// - The inspector offers a photo's Exposure, Contrast, Saturation, and Warmth, each a slider.
// - Saturation let go at its least is one `choose`, written on the photo; where the photo was
//   most colorful it is now gray, and the slide beside it is as it was.
// - Warmth let go at its most is another, and the photo warms toward orange; one undo each, and
//   the deck is as it was.
// - Border chosen (PLAN 2.118) is one `choose` of the photo's `stroke`, the accent drawn on the
//   edge of the picture that shows; one undo, and the deck is as it was.
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
  const settled = (before) =>
    page.waitForFunction((n) => window.scaena.trips().length > n && !window.scaena.canvas.busy(), before, { timeout: 60000, polling: 50 });
  const declared = (scn, node) => {
    const lines = scn.split("\n");
    const first = lines.findIndex((l) => l.trim().startsWith(`${node} `));
    if (first < 0) return "";
    const deeper = (l) => l.search(/\S/) > lines[first].search(/\S/);
    let last = first + 1;
    while (last < lines.length && deeper(lines[last])) last++;
    return lines.slice(first, last).join(" ");
  };
  const undo = async () => {
    const n = await trips();
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+z");
    await settled(n);
  };
  const choose = async (node) => {
    await page.evaluate((n) => window.scaena.canvas.select(n), node);
    await page.waitForFunction((n) => window.scaena.look.offered()?.node === n, node, { timeout: 30000 });
  };
  /** The slide shown as the PNG export draws it, 960 wide: its colors at canvas points `at`. */
  const pixels = async (at) => {
    const [cw] = await page.evaluate(() => window.scaena.canvas.size());
    return page.evaluate(
      async ([points, scale]) => {
        const shot = await window.scaena.exportAs("png", 960);
        const bitmap = await createImageBitmap(new Blob([shot.bytes], { type: "image/png" }));
        const g = new OffscreenCanvas(bitmap.width, bitmap.height).getContext("2d");
        g.drawImage(bitmap, 0, 0);
        return points.map((p) => [...g.getImageData(Math.round(p[0] * scale), Math.round(p[1] * scale), 1, 1).data].slice(0, 3));
      },
      [at, 960 / cw],
    );
  };
  const color = ([r, g, b]) => Math.max(r, g, b) - Math.min(r, g, b);
  /** Lets the slider `id` go at `value`, as a person lets one go. */
  const slide = async (id, value) => {
    const n = await trips();
    await page.evaluate(
      ([selector, v]) => {
        const input = document.querySelector(selector);
        input.value = String(v);
        input.dispatchEvent(new Event("input", { bubbles: true }));
        input.dispatchEvent(new Event("change", { bubbles: true }));
      },
      [id, value],
    );
    await settled(n);
  };

  const index = await page.evaluate(() => window.scaena.opened.states.indexOf("adjust"));
  const at = (await source()).indexOf("state adjust") + "state ".length;
  await page.evaluate((offset) => window.scaena.cursor(offset), at);
  await page.waitForFunction((i) => window.scaena.shown() === i && window.scaena.at().index === i, index, { timeout: 30000 });
  await page.waitForFunction(() => window.scaena.canvas.boxed() === "adjust" && window.scaena.canvas.boxes().length > 0, null, { timeout: 30000 });
  const original = await source();

  // The inspector offers the four, each a slider.
  await choose("adj-plain");
  await page.waitForSelector("#look-adjust-saturation", { timeout: 30000 });
  const sliders = await page.evaluate(() =>
    ["exposure", "contrast", "saturation", "warmth"].map((k) => document.querySelector(`#look-adjust-${k}`)?.type ?? "none"),
  );
  check(sliders.every((t) => t === "range"), `Exposure, Contrast, Saturation, and Warmth are sliders: ${sliders.join(", ")}`);
  const labels = await page.evaluate(() => document.querySelector("#look").textContent);
  check(["Exposure", "Contrast", "Saturation", "Warmth"].every((w) => labels.includes(w)), "each in a person's words");

  // Where the photo is most colorful, on a grid across its box; and a point on the slide beside it.
  const [x, y, w, h] = await page.evaluate(() => window.scaena.canvas.boxes().find((b) => b.node === "adj-plain").rect);
  const grid = [];
  for (let i = 1; i < 8; i++) for (let j = 1; j < 8; j++) grid.push([x + (w * i) / 8, y + (h * j) / 8]);
  const before = await pixels(grid);
  const most = before.reduce((best, p, k) => (color(p) > color(before[best]) ? k : best), 0);
  const spot = grid[most];
  const beside = [x + w + 12, y + h / 2];
  check(color(before[most]) > 60, `the photo has color to take away: ${before[most]}`);
  const [besideBefore] = await pixels([beside]);

  // Saturation at its least: one `choose`, written on the photo, and the color is gone.
  await slide("#look-adjust-saturation", -1);
  check(/\bsaturation:\s*-1\b/.test(declared(await source(), "adj-plain")), `written on the photo: ${declared(await source(), "adj-plain")}`);
  check((await status()).length > 0, `the status says what changed: ${await status()}`);
  await page.waitForTimeout(300);
  const [gray, besideAfter] = await pixels([spot, beside]);
  check(color(gray) <= 6, `where it was most colorful it is gray: ${before[most]} → ${gray}`);
  check(JSON.stringify(besideAfter) === JSON.stringify(besideBefore), `the slide beside it is as it was: ${besideBefore} → ${besideAfter}`);
  await undo();
  check((await source()) === original, "one undo takes the saturation back");

  // Warmth at its most: the photo warms toward orange, red up and blue down across it. (On a
  // photo already in black and white it stays so: saturation comes after the white balance.)
  await slide("#look-adjust-warmth", 1);
  check(/\bwarmth:\s*1\b/.test(declared(await source(), "adj-plain")), `warmth too: ${declared(await source(), "adj-plain")}`);
  await page.waitForTimeout(300);
  const warm = await pixels(grid);
  const lean = (ps) => ps.reduce((sum, [r, , b]) => sum + r - b, 0) / ps.length;
  check(lean(warm) > lean(before) + 10, `and it warms toward orange: red over blue ${lean(before).toFixed(1)} → ${lean(warm).toFixed(1)}`);
  await undo();
  check((await source()) === original, "and one undo takes the warmth back: the deck is as it was");
  check(await page.evaluate(() => window.scaena.last().valid), "and it validates");

  // A border (PLAN 2.118): the accent on the edge of the picture that shows.
  await choose("adj-plain");
  await page.waitForFunction(() => window.scaena.canvas.framing()?.node === "adj-plain", null, { timeout: 30000 });
  const [sx, sy, sw] = await page.evaluate(() => window.scaena.canvas.framing().shown);
  const edge = [-2, -1, 0, 1, 2].map((d) => [sx + sw / 2, sy + d]);
  const red = (points) => Math.max(...points.map(([r, , b]) => r - b));
  const edgeBefore = red(await pixels(edge));
  await page.waitForSelector("#look-stroke-paint", { timeout: 30000 });
  let n = await trips();
  await page.selectOption("#look-stroke-paint", "accent");
  await settled(n);
  check(/\bstroke\b[^\n]*accent/.test(declared(await source(), "adj-plain")), `a border written on the photo: ${declared(await source(), "adj-plain")}`);
  await page.waitForTimeout(300);
  const edgeAfter = red(await pixels(edge));
  check(edgeAfter > edgeBefore + 40, `the accent drawn on the picture's edge: ${edgeBefore} → ${edgeAfter}`);
  await undo();
  check((await source()) === original, "one undo takes the border away");
} catch (e) {
  failures.push(`error: ${e.message}`);
} finally {
  await browser.close();
  server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "an image's light and color are adjusted from the inspector");
process.exit(failures.length ? 1 : 0);

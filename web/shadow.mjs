// PLAN 2.115 check: an object's shadow from the inspector, in headless Chromium (serve.mjs), the
// CPU painting, on the torture deck's `shadows` state.
//
//   node web/shadow.mjs     (after `just web`; from the repository's root)
//
// - The inspector offers a shape's Shadow: the theme's shadows, in plain words.
// - Raised chosen on the pill, which casts none, is one `choose`, written on the pill; the slide
//   under the pill's foot darkens where the shadow falls, and the pill itself is as it was.
// - Floating chosen in its place is another, and its shadow falls further; one undo each, and the
//   deck is as it was.
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
  /** The slide shown as the PNG export draws it, 960 wide: its color at canvas point `at`. */
  const pixel = async (at) => {
    const [cw] = await page.evaluate(() => window.scaena.canvas.size());
    return page.evaluate(
      async ([p, scale]) => {
        const shot = await window.scaena.exportAs("png", 960);
        const bitmap = await createImageBitmap(new Blob([shot.bytes], { type: "image/png" }));
        const g = new OffscreenCanvas(bitmap.width, bitmap.height).getContext("2d");
        g.drawImage(bitmap, 0, 0);
        return [...g.getImageData(Math.round(p[0] * scale), Math.round(p[1] * scale), 1, 1).data].slice(0, 3);
      },
      [at, 960 / cw],
    );
  };
  const light = ([r, g, b]) => r + g + b;

  const index = await page.evaluate(() => window.scaena.opened.states.indexOf("shadows"));
  const at = (await source()).indexOf("state shadows") + "state ".length;
  await page.evaluate((offset) => window.scaena.cursor(offset), at);
  await page.waitForFunction((i) => window.scaena.shown() === i && window.scaena.at().index === i, index, { timeout: 30000 });
  await page.waitForFunction(() => window.scaena.canvas.boxed() === "shadows" && window.scaena.canvas.boxes().length > 0, null, { timeout: 30000 });
  const original = await source();

  // The inspector offers the theme's shadows.
  await choose("sh-pill");
  await page.waitForSelector("#look-shadow", { timeout: 30000 });
  const offered = await page.evaluate(() => [...document.querySelectorAll("#look-shadow option")].map((o) => o.textContent));
  check(["Soft", "Raised", "Floating"].every((w) => offered.includes(w)), `Shadow offers the theme's shadows: ${offered.join(", ")}`);

  // Raised: one `choose`, written on the pill, its shadow under its foot.
  const [x, y, w, h] = await page.evaluate(() => window.scaena.canvas.boxes().find((b) => b.node === "sh-pill").rect);
  const under = [x + w / 2, y + h + 8];
  const inside = [x + w / 2, y + h / 2];
  const [underBefore, insideBefore] = [await pixel(under), await pixel(inside)];
  let n = await trips();
  await page.selectOption("#look-shadow", "raised");
  await settled(n);
  check(/\bshadow:raised\b/.test(declared(await source(), "sh-pill")), `written on the pill: ${declared(await source(), "sh-pill")}`);
  check((await status()).length > 0, `the status says what changed: ${await status()}`);
  await page.waitForTimeout(300);
  const [underRaised, insideRaised] = [await pixel(under), await pixel(inside)];
  check(light(underRaised) < light(underBefore) - 20, `the slide under its foot darkens: ${underBefore} → ${underRaised}`);
  check(JSON.stringify(insideRaised) === JSON.stringify(insideBefore), `the pill is as it was: ${insideBefore} → ${insideRaised}`);
  const raised = await source();

  // Floating in its place: it falls further.
  n = await trips();
  await page.selectOption("#look-shadow", "floating");
  await settled(n);
  check(/\bshadow:floating\b/.test(declared(await source(), "sh-pill")), `floating in its place: ${declared(await source(), "sh-pill")}`);
  await page.waitForTimeout(300);
  const further = [x + w / 2, y + h + 20];
  check(light(await pixel(further)) < light(underBefore) - 20, `and its shadow falls further: ${await pixel(further)}`);

  // One undo each.
  await undo();
  check((await source()) === raised, "one undo takes floating back");
  await undo();
  check((await source()) === original, "and another raised: the deck is as it was");
  check(await page.evaluate(() => window.scaena.last().valid), "and it validates");
} catch (e) {
  failures.push(`error: ${e.message}`);
} finally {
  await browser.close();
  server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "an object casts the theme's shadow from the inspector");
process.exit(failures.length ? 1 : 0);

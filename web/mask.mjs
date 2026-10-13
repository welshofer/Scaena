// PLAN 2.113 check: an image masked with a shape from the inspector, in headless Chromium
// (serve.mjs), the CPU painting, on the torture deck's `images` state.
//
//   node web/mask.mjs     (after `just web`; from the repository's root)
//
// - The inspector offers an image's Mask: an oval and the shape library's shapes, in plain words.
// - Star chosen is one `choose`, written on the image; the picture's corners are no longer drawn
//   (the canvas's pixels there are the slide's ground), and its middle still is.
// - Oval chosen in its place is another; one undo each, and the deck is as it was.
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
  const says = (what) =>
    page.waitForFunction((w) => document.querySelector("#status").textContent.includes(w), what, { timeout: 30000 }).then(
      () => true,
      () => false,
    );
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

  const index = await page.evaluate(() => window.scaena.opened.states.indexOf("images"));
  const at = (await source()).indexOf("state images") + "state ".length;
  await page.evaluate((offset) => window.scaena.cursor(offset), at);
  await page.waitForFunction((i) => window.scaena.shown() === i && window.scaena.at().index === i, index, { timeout: 30000 });
  await page.waitForFunction(() => window.scaena.canvas.boxed() === "images" && window.scaena.canvas.boxes().length > 0, null, { timeout: 30000 });
  const original = await source();

  // The inspector offers a mask: an oval and the library's shapes.
  await choose("image-cover");
  await page.waitForSelector("#look-mask", { timeout: 30000 });
  const offered = await page.evaluate(() => [...document.querySelectorAll("#look-mask option")].map((o) => o.textContent));
  const wanted = ["Oval", "Star", "Hexagon", "Diamond", "Right Arrow"];
  check(wanted.every((w) => offered.includes(w)), `Mask offers an oval and the library's shapes: ${offered.join(", ")}`);

  // Star: one `choose`, written on the image.
  const [x, y, w, h] = await page.evaluate(() => window.scaena.canvas.boxes().find((b) => b.node === "image-cover").rect);
  const corner = [x + 2, y + 2];
  const middle = [x + w / 2, y + h / 2];
  const [cornerBefore, middleBefore] = [await pixel(corner), await pixel(middle)];
  let n = await trips();
  await page.selectOption("#look-mask", "star");
  await settled(n);
  check(/\bmask:star\b/.test(declared(await source(), "image-cover")), `written on the image: ${declared(await source(), "image-cover")}`);
  check((await status()).length > 0, `the status says what changed: ${await status()}`);
  await page.waitForTimeout(300);
  const [cornerAfter, middleAfter] = [await pixel(corner), await pixel(middle)];
  check(JSON.stringify(cornerAfter) !== JSON.stringify(cornerBefore), `its corner is no longer the picture: ${cornerBefore} → ${cornerAfter}`);
  check(JSON.stringify(middleAfter) === JSON.stringify(middleBefore), `its middle still is: ${middleBefore} → ${middleAfter}`);
  const starred = await source();

  // Oval in its place.
  n = await trips();
  await page.selectOption("#look-mask", "ellipse");
  await settled(n);
  check(/\bmask:ellipse\b/.test(declared(await source(), "image-cover")), `an oval in its place: ${declared(await source(), "image-cover")}`);

  // One undo each.
  await undo();
  check((await source()) === starred, "one undo takes the oval back");
  await undo();
  check((await source()) === original, "and another the star: the deck is as it was");
  check(await page.evaluate(() => window.scaena.last().valid), "and it validates");
} catch (e) {
  failures.push(`error: ${e.message}`);
} finally {
  await browser.close();
  server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "an image is masked with a shape from the inspector");
process.exit(failures.length ? 1 : 0);

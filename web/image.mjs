// PLAN 2.45 check: an image from the inspector, in headless Chromium (serve.mjs), the CPU painting,
// on the torture deck's `images` state.
//
//   node web/image.mjs     (after `just web`; from the repository's root)
//
// - The inspector offers an image's fit, its focal point and crop as fractions, and its radius.
// - Pick, then a click on the image, sets its focal point to the point of the image drawn there:
//   one `choose`, and one undo. Escape leaves it as it was.
// - Its crop and its fit, chosen in the inspector, are each one patch; a crop is kept inside the image.
// - A PNG dropped on the image takes its place: the file joins the bundle, named by its SHA-256,
//   and the image's `src` is its path. So does a JPEG, as it is (PLAN 2.66), drawn as its EXIF
//   orientation turns it. Dropped on a text, it is inserted there, as Insert inserts it (PLAN
//   2.76), and one undo takes it out. A file neither a picture nor data changes nothing.
// - Undone, the deck is as it was, and it validates.
// Exits 1 on any failure.
import { readFileSync } from "node:fs";
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
  /** The value the inspector shows for the node selected's `prop`. */
  const shows = (prop) => page.evaluate((p) => window.scaena.look.offered()?.fields.find((f) => f.prop === p)?.value, prop);
  /** `node`'s declaration in the source: its first line and those after it set deeper. */
  const declared = (scn, node) => {
    const lines = scn.split("\n");
    const first = lines.findIndex((l) => l.trim().startsWith(`${node} `));
    if (first < 0) return "";
    const deeper = (l) => l.search(/\S/) > lines[first].search(/\S/);
    let last = first + 1;
    while (last < lines.length && deeper(lines[last])) last++;
    return lines.slice(first, last).join(" ");
  };
  /** Where canvas point `at` is on the page. */
  const client = async ([x, y]) => {
    const r = await page.locator("#overlay").boundingBox();
    const [w, h] = await page.evaluate(() => window.scaena.canvas.size());
    return [r.x + (x / w) * r.width, r.y + (y / h) * r.height];
  };
  const undo = async () => {
    const n = await trips();
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+z");
    await settled(n);
  };
  /** Select `node` and wait for the inspector to offer what it has. */
  const choose = async (node) => {
    await page.evaluate((n) => window.scaena.canvas.select(n), node);
    await page.waitForFunction((n) => window.scaena.look.offered()?.node === n, node, { timeout: 30000 });
  };

  const index = await page.evaluate(() => window.scaena.opened.states.indexOf("images"));
  const at = (await source()).indexOf("state images") + "state ".length;
  await page.evaluate((offset) => window.scaena.cursor(offset), at);
  await page.waitForFunction((i) => window.scaena.shown() === i && window.scaena.at().index === i, index, { timeout: 30000 });
  await page.waitForFunction(() => window.scaena.canvas.boxed() === "images" && window.scaena.canvas.boxes().length > 0, null, { timeout: 30000 });
  const original = await source();

  // The inspector offers an image's look: fit, focal point, crop, radius.
  await choose("image-cover");
  const props = await page.evaluate(() => window.scaena.look.offered().fields.map((f) => f.prop));
  check(["fit", "focal", "crop", "radius"].every((p) => props.includes(p)), `an image's fields: ${props}`);
  check((await page.locator('#look input[data-prop="focal"][data-part]').count()) === 2, "its focal point, x and y");
  check((await page.locator('#look input[data-prop="crop"][data-part]').count()) === 4, "its crop, x, y, w, and h");

  // Pick, then a click on the image a quarter of the way across: the point of the image drawn there.
  const rect = await page.evaluate(() => window.scaena.canvas.boxes().find((b) => b.node === "image-cover").rect);
  const [x, y, w, h] = rect;
  // The test card is 480 × 240, covering the box around its middle.
  const s = Math.max(w / 480, h / 240);
  const [sw, sh] = [w / s, h / s];
  const expected = [((480 - sw) / 2 + 0.25 * sw) / 480, ((240 - sh) / 2 + 0.5 * sh) / 240];
  let n = await trips();
  await page.locator("#look [data-pick]").click();
  check(await says("click image-cover where its subject is"), `Pick asks for a click on the image: ${await status()}`);
  const [cx, cy] = await client([x + w * 0.25, y + h * 0.5]);
  await page.mouse.click(cx, cy);
  check(await says("image-cover's focal point"), `a click sets its focal point: ${await status()}`);
  await settled(n);
  await page.waitForFunction(() => Array.isArray(window.scaena.look.offered()?.fields.find((f) => f.prop === "focal")?.value), null, { timeout: 30000 }).catch(() => {});
  const focal = await shows("focal");
  check(
    Array.isArray(focal) && Math.abs(focal[0] - expected[0]) < 0.01 && Math.abs(focal[1] - expected[1]) < 0.01,
    `to the point of the image under the click: ${JSON.stringify(focal)}, ${expected.map((v) => v.toFixed(3))} expected`,
  );
  check(/\bfocal:\[/.test(declared(await source(), "image-cover")), `written on the image: ${declared(await source(), "image-cover")}`);
  check((await page.evaluate(() => window.scaena.canvas.chosen())).join() === "image-cover", "and the image stays selected");
  await undo();
  check((await source()) === original, "one undo takes it back");

  // Escape leaves the focal point as it is.
  await choose("image-cover");
  await page.locator("#look [data-pick]").click();
  await page.keyboard.press("Escape");
  check(await says("the focal point is as it was"), `Escape leaves it: ${await status()}`);
  check((await source()) === original, "and nothing changes");

  // Its crop, from the inspector: the left half.
  await choose("image-cover");
  n = await trips();
  // As the inspector's own test sets a value: once, so no later blur sends it again.
  await page.evaluate(() => {
    const input = document.querySelector('#look input[data-prop="crop"][data-part="2"]');
    input.value = "0.5";
    input.dispatchEvent(new Event("change", { bubbles: true }));
  });
  check(await says("image-cover's crop"), `the crop is chosen: ${await status()}`);
  await settled(n);
  check(/\bcrop:\[0, ?0, ?0\.5, ?1\]/.test(declared(await source(), "image-cover")), `written: ${declared(await source(), "image-cover")}`);
  await undo();
  check((await source()) === original, "one undo takes it back");

  // A crop past the image's edge is kept inside it: from 0.6 across, the 0.4 left.
  await choose("image-cover");
  n = await trips();
  await page.evaluate(() => {
    const input = document.querySelector('#look input[data-prop="crop"][data-part="0"]');
    input.value = "0.6";
    input.dispatchEvent(new Event("change", { bubbles: true }));
  });
  await settled(n);
  check(/\bcrop:\[0\.6, ?0, ?0\.4, ?1\]/.test(declared(await source(), "image-cover")), `a crop past the edge is kept inside it: ${declared(await source(), "image-cover")}`);
  await undo();
  check((await source()) === original, "one undo takes it back");

  // Its fit.
  await choose("image-cover");
  n = await trips();
  await page.locator('#look select[data-prop="fit"]').selectOption("contain");
  check(await says("image-cover's fit"), `the fit is chosen: ${await status()}`);
  await settled(n);
  check(/\bfit:contain\b/.test(declared(await source(), "image-cover")), `written: ${declared(await source(), "image-cover")}`);
  await undo();
  check((await source()) === original, "one undo takes it back");

  // A PNG dropped on the image takes its place.
  const png = [...readFileSync("docs/examples/assets/trails-ridge.png")];
  const jpeg = [...readFileSync("tests/fixtures/jpeg/orientation-6.jpg")];
  /** A drop of `bytes` (the PNG's by default) as file `name` on canvas point `at`. */
  const drop = async (name, at, bytes = png) => {
    const transfer = await page.evaluateHandle(
      ([b, file]) => {
        const type = file.endsWith(".png") ? "image/png" : file.endsWith(".jpg") ? "image/jpeg" : "text/plain";
        const dt = new DataTransfer();
        dt.items.add(new File([new Uint8Array(b)], file, { type }));
        return dt;
      },
      [bytes, name],
    );
    const [px, py] = await client(at);
    await page.dispatchEvent("#overlay", "drop", { dataTransfer: transfer, clientX: px, clientY: py });
  };
  // On a text, a picture is inserted there, as Insert inserts it (PLAN 2.76); one undo takes it out.
  const label = await page.evaluate(() => window.scaena.canvas.boxes().find((b) => b.node === "case").rect);
  n = await trips();
  await drop("ridge.png", [label[0] + label[2] / 2, label[1] + label[3] / 2]);
  check(await says("ridge.png inserted as image-"), `a PNG dropped on a text is inserted there: ${await status()}`);
  await settled(n);
  check(/\bimage-[0-9a-f]+ image\s+"assets\/[0-9a-f]{64}\.png"/.test(await source()), "a new image of it");
  await undo();
  check((await source()) === original, "one undo takes it out");
  // A file neither a picture nor data: nothing changes, and the status says why.
  await drop("ridge.txt", [x + w / 2, y + h / 2]);
  check(await says("ridge.txt is neither a picture (PNG, JPEG) nor data (CSV, JSON)"), `a file that is neither, on the image: ${await status()}`);
  check((await source()) === original, "and it does not change the deck");
  n = await trips();
  await drop("ridge.png", [x + w / 2, y + h / 2]);
  check(await says("image-cover shows ridge.png, kept as assets/"), `a PNG dropped on it takes its place: ${await status()}`);
  await settled(n);
  const src = declared(await source(), "image-cover").match(/\bimage "([^"]+)"/)?.[1] ?? "";
  check(/^assets\/[0-9a-f]{64}\.png$/.test(src), `named by its SHA-256: ${src}`);
  check(await page.evaluate(() => window.scaena.last().valid), "the deck validates with it");
  await undo();
  check((await source()) === original, "one undo puts the test card back");
  check(await page.evaluate(() => window.scaena.last().valid), "and the deck still validates");

  // A photo dropped on it takes its place as it is: a JPEG, named by its SHA-256. (How the browser
  // draws it, turned upright, the web smoke holds to torture case 50's golden.)
  n = await trips();
  await drop("turned.jpg", [x + w / 2, y + h / 2], jpeg);
  check(await says("image-cover shows turned.jpg, kept as assets/"), `a JPEG dropped on it takes its place: ${await status()}`);
  await settled(n);
  const photo = declared(await source(), "image-cover").match(/\bimage "([^"]+)"/)?.[1] ?? "";
  check(/^assets\/[0-9a-f]{64}\.jpg$/.test(photo), `named by its SHA-256, a JPEG still: ${photo}`);
  check(await page.evaluate(() => window.scaena.last().valid), "the deck validates with it");
  await undo();
  check((await source()) === original, "one undo puts the test card back again");
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

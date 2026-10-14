// PLAN 2.74 check: an image's crop and focal point on the canvas, in the editor in headless
// Chromium (serve.mjs), the CPU painting, on the torture deck's `images` case.
//
//   node web/crop.mjs     (after `just web`; from the repository's root)
//
// - An image selected has a crop handle inside each side of what shows, and its focal point where
//   it is drawn.
// - A crop handle dragged shows the whole image outlined, and the part the crop keeps, its edge at
//   the pointer, as it goes; let go, the picture's edge has come in that far and the rest of it is
//   where it was: the crop and its box, one patch, one step to undo. Escape through a drag leaves
//   it as it was, and Alt keeps it to the state shown, which its undo leaves shown.
// - The focal point dragged is one `choose` of `focal`. Where it changes nothing, a picture filling
//   its box, no handle for it stands in the picture's middle, and a drag from there moves it.
// - axe-core finds nothing with the handles shown, and the keys sheet lists the gestures.
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
  await page.goto(`${site.origin}/web/dist/editor.html?painter=cpu&bundle=/tests/fixtures/torture.scaena/deck.json`);
  await page.waitForFunction(() => window.scaena?.last()?.valid, null, { timeout: 120000 });
  await page.selectOption("#state", "images");
  await page.waitForFunction(() => window.scaena.canvas.boxed() === "images", null, { timeout: 30000 }).catch(() => {});

  const source = () => page.evaluate(() => window.scaena.source());
  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  const says = async (text) => {
    await page.waitForFunction((t) => document.querySelector("#status").textContent.includes(t), text, { timeout: 30000 }).catch(() => {});
    return (await status()).includes(text);
  };
  const back = (to) =>
    page
      .waitForFunction((s) => window.scaena.source() === s, to, { timeout: 30000, polling: 50 })
      .then(() => true)
      .catch(() => false);
  /** `node` selected, once its framing has come: with `shown`, the framing whose part shown that
   * is. After an undo the canvas asks for the framing again, and until the answer comes it keys its
   * handles on the one it holds, the crop's, whose box came in with it, so no focal point stood in it. */
  const choose = async (node, shown) => {
    await page.evaluate((n) => window.scaena.canvas.select(n), node);
    await page
      .waitForFunction(
        ([n, s]) => {
          const f = window.scaena.canvas.framing();
          return f?.node === n && (s === undefined || JSON.stringify(f.shown) === s) && document.querySelector("#overlay svg [data-crop]");
        },
        [node, shown && JSON.stringify(shown)],
        { timeout: 30000 },
      )
      .catch(() => {});
    return page.evaluate(() => window.scaena.canvas.framing());
  };
  /** A point of the overlay, canvas units, on the page. */
  const onPage = ([x, y]) =>
    page.evaluate(
      ([x, y]) => {
        const r = document.querySelector("#overlay").getBoundingClientRect();
        const [w, h] = window.scaena.canvas.size();
        return [r.left + (x / w) * r.width, r.top + (y / h) * r.height];
      },
      [x, y],
    );
  /** The middle of the handle `selector` picks, canvas units. */
  const handle = (selector) =>
    page.evaluate((s) => {
      const el = document.querySelector(`#overlay svg ${s}`);
      if (!el) return null;
      const b = el.getBBox();
      return [b.x + b.width / 2, b.y + b.height / 2];
    }, selector);
  const drag = async (from, to, { mid, alt } = {}) => {
    const [fx, fy] = await onPage(from);
    await page.mouse.move(fx, fy);
    if (alt) await page.keyboard.down("Alt");
    await page.mouse.down();
    for (let k = 1; k <= 8; k++) {
      const [x, y] = await onPage([from[0] + ((to[0] - from[0]) * k) / 8, from[1] + ((to[1] - from[1]) * k) / 8]);
      await page.mouse.move(x, y);
    }
    if (mid) await mid();
    await page.mouse.up();
    if (alt) await page.keyboard.up("Alt");
  };
  const comes = (text) =>
    page
      .waitForFunction((t) => window.scaena.source().includes(t), text, { timeout: 30000, polling: 50 })
      .then(() => true)
      .catch(() => false);

  const original = await source();
  // The cover image: what shows fills its box.
  const cover = await choose("image-cover");
  check(cover?.fit === "cover" && JSON.stringify(cover.crop) === "[0,0,1,1]", `the image's framing: ${JSON.stringify(cover)}`);
  check((await page.locator("#overlay svg [data-crop]").count()) === 4 && (await page.locator("#overlay svg .handle.focal").count()) === 1, "a crop handle inside each side, and the focal point");
  await page.addScriptTag({ path: axe });
  const found = await page.evaluate(async () => {
    const result = await window.axe.run(document, { runOnly: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"] });
    return result.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
  });
  check(!found.length, `axe finds nothing with the handles shown${found.length ? `:\n    ${found.join("\n    ")}` : ""}`);

  // The right side's handle, dragged in a quarter of what shows: the picture's right edge comes in
  // with the pointer, from where the handle stood, and the rest of it stays where it was drawn,
  // as a presentation app's crop does: the crop and the box, one patch.
  const right = await handle('[data-crop="e"]');
  const by = cover.shown[2] / 4;
  let during = null;
  await drag(right, [right[0] - by, right[1]], {
    mid: async () => {
      during = await page.evaluate(() => {
        const frame = document.querySelector("#overlay svg .cropping .crop-frame");
        return [document.querySelectorAll("#overlay svg .cropping .image-whole").length, frame ? frame.getBBox().x + frame.getBBox().width : null];
      });
    },
  });
  check(during?.[0] === 1 && Math.abs(during[1] - (cover.shown[0] + cover.shown[2] - by)) < 3, `as it is dragged, the whole image is outlined, and the kept part's edge follows the pointer: ${JSON.stringify(during)}`);
  const trimmed = await page
    .waitForFunction((w) => { const f = window.scaena.canvas.framing(); return f?.node === "image-cover" && Math.abs(f.shown[2] - w) > 2 ? f : null; }, cover.shown[2], { timeout: 30000 })
    .then((h) => h.jsonValue())
    .catch(() => null);
  const near = (a, b) => a.every((v, i) => Math.abs(v - b[i]) < 2.5);
  check(
    trimmed !== null && Math.abs(trimmed.shown[0] - cover.shown[0]) < 2.5 && Math.abs(trimmed.shown[0] + trimmed.shown[2] - (cover.shown[0] + cover.shown[2] - by)) < 2.5,
    `let go, the right edge of what shows came in ${Math.round(by)} units, the left where it was: ${JSON.stringify(cover.shown)} → ${JSON.stringify(trimmed?.shown)}`,
  );
  check(trimmed !== null && near(trimmed.whole, cover.whole), `the rest of the picture stays where it was drawn: ${JSON.stringify(cover.whole)} → ${JSON.stringify(trimmed?.whole)}`);
  // The node's declaration, its first line and those set deeper after it.
  const declared = (scn, node) => {
    const lines = scn.split("\n");
    const first = lines.findIndex((l) => l.trim().startsWith(`${node} `));
    if (first < 0) return "";
    let last = first + 1;
    while (last < lines.length && lines[last].search(/\S/) > lines[first].search(/\S/)) last++;
    return lines.slice(first, last).join(" ");
  };
  const line = declared(await source(), "image-cover");
  check(/crop:\[/.test(line) && /rect\(/.test(line), `the crop and its box written: ${line}`);
  check(await says("cropped to"), `the status says so: ${await status()}`);
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+z");
  check(await back(original), "one undo takes back the crop and the box together");

  // Escape through a drag: as it was.
  const top = await handle('[data-crop="n"]');
  await drag(top, [top[0], top[1] + cover.whole[3] / 5], { mid: () => page.keyboard.press("Escape") });
  await page.waitForTimeout(300);
  check((await source()) === original && (await says("stay as they are")), `Escape leaves the crop as it was: ${await status()}`);

  // With Alt, kept to the state shown.
  await choose("image-cover");
  const left = await handle('[data-crop="w"]');
  await drag(left, [left[0] + cover.whole[2] / 10, left[1]], { alt: true });
  // The drag's own words say it too: the patch is in once the source has it.
  const forked = await page
    .waitForFunction((was) => window.scaena.source() !== was, original, { timeout: 30000, polling: 50 })
    .then(() => true)
    .catch(() => false);
  check(forked && (await says("kept to this slide")) && /images[\s\S]*image-cover crop:/.test(await source()), `with Alt, kept to the slide shown: ${await status()}`);
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+z");
  await back(original);
  // The undo writes over where images starts in the source, which takes it out of the editor's
  // table of states until the source is compiled again: the slide shown stays images all along,
  // read once the table has it again.
  await page.waitForFunction(() => window.scaena.last().states.some(([id]) => id === "images"), null, { timeout: 30000 }).catch(() => {});
  const after = await page.evaluate(() => window.scaena.last().states[window.scaena.shown()]?.[0]);
  check(after === "images", `one undo takes it back on its slide, still shown: ${after}`);
  await page.waitForFunction(() => window.scaena.canvas.boxed() === "images", null, { timeout: 30000 }).catch(() => {});

  // The focal point, dragged to the left edge of what shows: the image's left in view.
  const framing = await choose("image-cover", cover.shown);
  const focal = await handle(".handle.focal");
  await drag(focal, [framing.shown[0] + 1, focal[1]]);
  const moved = await page
    .waitForFunction(() => window.scaena.canvas.framing()?.focal[0] < 0.1, null, { timeout: 30000 })
    .then(() => page.evaluate(() => window.scaena.canvas.framing()?.focal))
    .catch(() => null);
  check(moved !== null && Math.abs(moved[1] - 0.5) < 0.01, `the focal point moves where it is dropped: ${JSON.stringify(moved)} · ${await status()}`);
  check((await source()).includes(`focal:[${moved?.join(", ")}]`), "one choose of focal");
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+z");
  check(await back(original), "one undo takes it back");
  await page.selectOption("#state", "images");
  await page.waitForFunction(() => window.scaena.canvas.boxed() === "images", null, { timeout: 30000 }).catch(() => {});

  // A picture its box shows as it is, filled: its focal point changes nothing, so no handle for it
  // stands in its middle, and a drag from its middle moves the picture.
  const fill = await choose("image-fill");
  check((await page.locator("#overlay svg .handle.focal").count()) === 0, `no focal point where it changes nothing: ${fill?.fit}`);
  const [fx, fy, fw, fh] = fill.rect;
  await drag([fx + fw / 2, fy + fh / 2], [fx + fw / 2, fy + fh / 2 + 200]);
  const placed = await page
    .waitForFunction((was) => (window.scaena.canvas.boxes().find((b) => b.node === "image-fill")?.rect[1] ?? was) !== was, fy, { timeout: 30000 })
    .then(() => true)
    .catch(() => false);
  check(placed && !/image-fill[^\n]*focal/.test(await source()), `a drag from its middle moves the picture: ${await status()}`);
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+z");
  check(await back(original), "one undo takes it back");

  // The keys sheet.
  await page.keyboard.press("Escape");
  await page.keyboard.press("?");
  await page.waitForSelector("#keys[open]", { timeout: 10000 }).catch(() => {});
  const sheet = await page.evaluate(() => [...document.querySelectorAll("#keys tr")].map((tr) => tr.textContent));
  check(sheet.some((l) => l.includes("Drag a crop handle inside an image")) && sheet.some((l) => l.includes("focal point")), "the keys sheet lists cropping and the focal point");
} finally {
  await browser.close();
  await site.close();
}

if (failures.length) {
  console.log(`\n${failures.length} failed:\n  ${failures.join("\n  ")}`);
  process.exit(1);
}
console.log("\nall passed");

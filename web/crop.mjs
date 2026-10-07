// PLAN 2.74 check: an image's crop and focal point on the canvas, in the editor in headless
// Chromium (serve.mjs), the CPU painting, on the torture deck's `images` case.
//
//   node web/crop.mjs     (after `just web`; from the repository's root)
//
// - An image selected has a crop handle inside each side of what shows, and its focal point where
//   it is drawn.
// - A crop handle dragged shows the whole image outlined, and the part the crop keeps, as it goes;
//   let go, it is one `choose` of `crop`, one step to undo. Escape through a drag leaves it as it
//   was, and Alt keeps it to the state shown.
// - The focal point dragged is one `choose` of `focal`.
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
  /** `node` selected, once its framing has come. */
  const choose = async (node) => {
    await page.evaluate((n) => window.scaena.canvas.select(n), node);
    await page.waitForFunction((n) => window.scaena.canvas.framing()?.node === n && document.querySelector("#overlay svg [data-crop]"), node, { timeout: 30000 }).catch(() => {});
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

  // The right side's handle, a quarter of the whole image's width in: the crop keeps three quarters.
  const right = await handle('[data-crop="e"]');
  let during = null;
  await drag(right, [right[0] - cover.whole[2] / 4, right[1]], {
    mid: async () => {
      during = await page.evaluate(() => [document.querySelectorAll("#overlay svg .cropping .image-whole").length, document.querySelectorAll("#overlay svg .cropping .crop-frame").length]);
    },
  });
  check(JSON.stringify(during) === "[1,1]", `as it is dragged, the whole image is outlined, and the part the crop keeps: ${JSON.stringify(during)}`);
  check(await comes("crop:[0, 0, 0.75, 1]"), `let go, the crop keeps three quarters of the image: ${(await source()).match(/image-cover[^\n]*/)?.[0]}`);
  check(await says("image-cover cropped to 0, 0, 0.75, 1"), `the status says so: ${await status()}`);
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+z");
  check(await back(original), "one undo takes it back");

  // Escape through a drag: as it was.
  const top = await handle('[data-crop="n"]');
  await drag(top, [top[0], top[1] + cover.whole[3] / 5], { mid: () => page.keyboard.press("Escape") });
  await page.waitForTimeout(300);
  check((await source()) === original && (await says("stay as they are")), `Escape leaves the crop as it was: ${await status()}`);

  // With Alt, kept to the state shown.
  await choose("image-cover");
  const left = await handle('[data-crop="w"]');
  await drag(left, [left[0] + cover.whole[2] / 10, left[1]], { alt: true });
  check(await says("kept to images"), `with Alt, kept to the state shown: ${await status()}`);
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+z");
  await back(original);
  // The undo puts the cursor where the change was, which may be another state: images again.
  await page.selectOption("#state", "images");
  await page.waitForFunction(() => window.scaena.canvas.boxed() === "images", null, { timeout: 30000 }).catch(() => {});

  // The focal point, dragged to the left edge of what shows: the image's left in view.
  const framing = await choose("image-cover");
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

// PLAN 2.62 check: every format at once, in the editor in headless Chromium (serve.mjs), the CPU
// painting, on the revenue example, which lists 16:9 and 9:16 besides its own canvas.
//
//   node web/formats.mjs     (after `just web`; from the repository's root)
//
// - Formats shows a figure for the deck's own canvas and for each format it lists, all one height,
//   each the shape of its format, the one the canvas shows pressed.
// - Each is the state shown as the canvas shows it in that format (the WASM session's test holds
//   the bytes to the canvas's), and follows the state shown.
// - The cue played, they are painted inside it as it plays, and come to rest with it.
// - Each counts the findings about the state shown that hold in its format: a headline that sets in
//   too many lines in 9:16 alone counts on 9:16's figure, and on no other.
// - A click on a figure, or Enter on one, opens the canvas in its format.
// - An edit that takes a format out of the deck takes its figure away, and one that puts it back
//   brings it back.
// - Hidden, it asks the worker to paint none.
// - axe-core finds nothing against WCAG 2.1 AA with the row shown.
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
  await page.goto(`${site.origin}/web/dist/editor.html?painter=cpu&bundle=/docs/examples/revenue.deck.json`);
  await page.waitForFunction(() => window.scaena?.last()?.valid, null, { timeout: 120000 });
  await page.selectOption("#state", "revenue");
  await page.waitForFunction(() => window.scaena.at()?.index === 1, null, { timeout: 30000 }).catch(() => {});

  const painted = () => page.evaluate(() => window.scaena.formats.painted());
  const figures = () =>
    page.locator("#formats button[data-format]").evaluateAll((els) =>
      els.map((e) => {
        const canvas = e.querySelector("canvas");
        const box = canvas.getBoundingClientRect();
        return {
          format: e.dataset.format,
          pressed: e.getAttribute("aria-pressed"),
          label: e.getAttribute("aria-label"),
          found: e.querySelector(".found")?.textContent ?? "",
          width: Math.round(box.width),
          height: Math.round(box.height),
        };
      }),
    );
  /** The figure for `format`, as a PNG of what it shows. */
  const figure = (format) => page.locator(`#formats button[data-format="${format}"] canvas`).screenshot();
  /** Wait until the figures were painted again since `count`, at `state` and at rest: at the end
   * of its cue, which the editor may play to show it, its figures painted as it goes. */
  const repainted = (count, state) =>
    page
      .waitForFunction(
        ([n, s]) => {
          const p = window.scaena.formats.painted();
          const span = window.scaena.last()?.slots.find((slot) => slot.state === s)?.span ?? 0;
          return (p?.count ?? 0) > n && p?.state === s && p.t >= span;
        },
        [count, state],
        { timeout: 30000 },
      )
      .catch(() => {});
  /** Whether `format`'s figure comes to differ from `before`: the worker's canvas reaches the page
   * a frame or so after it is painted. */
  const differs = async (format, before) => {
    for (let i = 0; i < 20; i++) {
      if (!(await figure(format)).equals(before)) return true;
      await page.waitForTimeout(100);
    }
    return false;
  };

  // Hidden: the worker is asked to paint none.
  check((await page.locator("#formats").isHidden()) && (await painted()) === undefined, "hidden, the formats are not painted");

  // Shown: a figure for the deck's own canvas and each format it lists, one height, each its shape.
  await page.click("#formats-open");
  await page.waitForFunction(() => window.scaena.formats.painted()?.state === "revenue", null, { timeout: 30000 }).catch(() => {});
  check((await page.locator("#formats-open").getAttribute("aria-pressed")) === "true", "Formats is pressed");
  const shown = await figures();
  check(JSON.stringify(shown.map((f) => f.format)) === JSON.stringify(["", "16:9", "9:16"]), `a figure for each: ${JSON.stringify(shown.map((f) => f.format))}`);
  const [own, wide, tall] = shown;
  check(shown.every((f) => f.height === shown[0].height && f.height > 0), `all one height: ${shown.map((f) => f.height).join(", ")}`);
  check(own.width > own.height && wide.width === own.width && tall.height > tall.width, `each the shape of its format: ${shown.map((f) => `${f.width}×${f.height}`).join(", ")}`);
  check(own.pressed === "true" && wide.pressed === "false" && tall.pressed === "false", "the one the canvas shows is pressed");
  check((await painted())?.state === "revenue", `they show the state shown: ${JSON.stringify(await painted())}`);

  // axe-core, with the row shown.
  await page.addScriptTag({ path: axe });
  const found = await page.evaluate(async () => {
    const result = await window.axe.run(document, { runOnly: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"] });
    return result.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
  });
  check(!found.length, `axe finds nothing against WCAG 2.1 AA with the formats shown${found.length ? `:\n    ${found.join("\n    ")}` : ""}`);

  // The cue played: the figures are painted inside it, as it plays, and come to rest with it.
  const span = await page.evaluate(() => window.scaena.last().slots.find((s) => s.state === "revenue").span);
  const seen = [];
  await page.exposeFunction("seen", (t) => seen.push(t));
  await page.evaluate(() => {
    const was = window.scaena.formats.painted()?.count ?? 0;
    const look = () => {
      const p = window.scaena.formats.painted();
      if (p && p.count > was) window.seen(p.t);
      if (window.scaena.at().playing || !p || p.count === was) requestAnimationFrame(look);
    };
    requestAnimationFrame(look);
  });
  await page.locator("#cue [data-play]").click();
  await page.waitForFunction(() => window.scaena.at().playing, null, { timeout: 10000 }).catch(() => {});
  await page.waitForFunction(() => !window.scaena.at().playing, null, { timeout: 30000 }).catch(() => {});
  await page.waitForTimeout(300);
  const inside = seen.filter((t) => t > 0 && t < span);
  check(inside.length > 0, `painted as the cue plays: ${inside.length} frames inside its ${span} ms`);
  check((await painted())?.t >= span, `and come to rest with it: ${JSON.stringify(await painted())}`);

  // Each counts the findings about the state shown that hold in its format: a headline that sets
  // in more lines than its role allows in 9:16 alone (W202, on the intro) counts there, on no other.
  let count = (await painted()).count;
  await page.selectOption("#state", "intro");
  await repainted(count, "intro");
  const was = await page.evaluate(() => window.scaena.source());
  // The intro's title, set in the display role.
  const longer = was.replace(/(title text role:display )"Q3 Review"/, '$1"Revenue doubled again this quarter"');
  check(longer !== was, "the intro's title is in the source");
  await page.evaluate((s) => window.scaena.type(s), longer);
  await page.waitForFunction(() => window.scaena.last()?.whole, null, { timeout: 60000 }).catch(() => {});
  // The preview follows the cursor, which the edit left on the title: the intro is shown again.
  await page.selectOption("#state", "intro");
  await page.waitForFunction(() => document.querySelector('#formats button[data-format="9:16"] .found'), null, { timeout: 60000 }).catch(() => {});
  const counted = await figures();
  check(counted[2].found === "1" && /1 warning/.test(counted[2].label), `9:16 counts its finding: ${counted[2].label}`);
  check(counted[0].found === "" && counted[1].found === "", `the others count none: ${counted[0].label}; ${counted[1].label}`);
  await page.evaluate((s) => window.scaena.type(s), was);
  await page.waitForFunction(() => window.scaena.last()?.whole, null, { timeout: 60000 }).catch(() => {});
  await page.selectOption("#state", "intro");
  await page.waitForFunction(() => !document.querySelector("#formats .found"), null, { timeout: 60000 }).catch(() => {});
  check((await figures()).every((f) => f.found === ""), "put back, none counts any");
  count = (await painted()).count;
  await page.selectOption("#state", "revenue");
  await repainted(count, "revenue");

  // A click on 9:16 opens the canvas in it.
  count = (await painted()).count;
  await page.locator('#formats button[data-format="9:16"]').click();
  await page.waitForFunction(() => document.querySelector("#format").value === "9:16", null, { timeout: 10000 }).catch(() => {});
  await repainted(count, "revenue");
  check((await page.locator("#format").inputValue()) === "9:16", "a click opens the canvas in 9:16");
  const pressed = await figures();
  check(pressed[2].pressed === "true" && pressed[0].pressed === "false", "and its figure is pressed");
  // The worker holds the canvas: its shape is what the page shows of it.
  await page.waitForFunction(() => { const r = document.querySelector("#stage").getBoundingClientRect(); return r.height > r.width; }, null, { timeout: 10000 }).catch(() => {});
  const size = await page.evaluate(() => { const r = document.querySelector("#stage").getBoundingClientRect(); return [Math.round(r.width), Math.round(r.height)]; });
  check(size[1] > size[0], `the canvas is 9:16's shape: ${size.join("×")}`);
  // Enter on the deck's own canvas opens it there.
  count = (await painted()).count;
  await page.locator('#formats button[data-format=""]').focus();
  await page.keyboard.press("Enter");
  await page.waitForFunction(() => document.querySelector("#format").value === "", null, { timeout: 10000 }).catch(() => {});
  check((await page.locator("#format").inputValue()) === "", "Enter on the own canvas's figure opens the canvas there");

  // Another state shown: the figures follow it.
  const before = await figure("9:16");
  count = (await painted()).count;
  await page.selectOption("#state", "mix");
  await repainted(count, "mix");
  check(await differs("9:16", before), "another state shown: the figures follow it");

  // An edit that takes 9:16 out of the deck takes its figure away; putting it back brings it back.
  const source = await page.evaluate(() => window.scaena.source());
  const one = source.replace(/formats(:| )[^\n]*9:16[^\n]*\n/, (line) => line.replace(/,?\s*9:16/, ""));
  check(one !== source, "the deck lists its formats in the source");
  await page.evaluate((s) => window.scaena.type(s), one);
  await page.waitForFunction(() => document.querySelectorAll("#formats button[data-format]").length === 2, null, { timeout: 30000 }).catch(() => {});
  check(JSON.stringify((await figures()).map((f) => f.format)) === JSON.stringify(["", "16:9"]), `9:16 taken out: ${JSON.stringify((await figures()).map((f) => f.format))}`);
  check(!(await page.locator("#format option").allTextContents()).includes("9:16"), "and the format menu no longer lists it");
  await page.evaluate((s) => window.scaena.type(s), source);
  await page.waitForFunction(() => document.querySelectorAll("#formats button[data-format]").length === 3, null, { timeout: 30000 }).catch(() => {});
  check((await figures()).length === 3 && (await page.locator("#format option").allTextContents()).includes("9:16"), "put back, its figure and its menu item are back");

  // Hidden again: none is painted.
  await page.click("#formats-open");
  count = (await painted()).count;
  await page.selectOption("#state", "intro");
  await page.waitForTimeout(800);
  check((await page.locator("#formats").isHidden()) && (await painted()).count === count, "hidden again, none is painted");
  await context.close();
} catch (e) {
  failures.push(String(e));
} finally {
  await browser.close();
  site.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "every format shows the state shown, as it plays, with its findings, and opens for editing");
process.exit(failures.length ? 1 : 0);

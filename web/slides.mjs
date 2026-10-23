// PLAN 2.97 check: the light table, in the editor, in headless Chromium (serve.mjs), the CPU
// painting, on the revenue example: its slides `intro`, `revenue` (with its step `mix`), and
// `close`.
//
//   node web/slides.mjs     (after `just web`; from the repository's root)
//
// - Slides shows every slide in place of the preview, each drawn at its last state, the slide
//   shown selected and focused; each says its number and its steps, and no id (PLAN 2.99).
// - Alt+← moves `close` before `revenue`, and a drag puts it before `intro`: one patch each, one
//   step to undo, and every state reads as it read.
// - ⌘D copies `revenue`, its step too, just after itself, the copy selected; each copy reads as
//   what it copies.
// - Delete takes `revenue` out, its step too; `close` reads as it read.
// - Enter shows a slide's first state in the canvas, and the preview back; Escape closes it, and
//   what it said of itself in the status goes with it.
// - The slider at its foot makes the slides larger, fewer to a row, each painted larger; in a
//   window of 1024 × 700 it stands below every slide, none showing under it or past it.
// - In that window the inspector gives way to the slide, and the palette's Hide the inspector and
//   Hide the animations give the slide their room; Show the inspector shows it again.
// - axe-core finds nothing against WCAG 2.1 AA in the light table.
// - On the trails deck, whose texts have layouts of their own in its other sizes, ⌘D copies its
//   photo slide too, and the status says so with no id.
// Exits 1 on any failure.
import { axeInto, launch, serve } from "./serve.mjs";

const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};

const site = await serve();
const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1 });
  const page = await context.newPage();
  page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
  page.on("console", (m) => m.type() === "error" && failures.push(`console: ${m.text()}`));
  await page.goto(`${site.origin}/web/dist/editor.html?painter=cpu&bundle=/docs/examples/revenue.deck.json`);
  await page.waitForFunction(() => window.scaena?.last()?.valid, null, { timeout: 120000 });

  const source = () => page.evaluate(() => window.scaena.source());
  const status = () => page.locator("#status").textContent();
  const says = (words) => page.waitForFunction((w) => document.querySelector("#status").textContent.includes(w), words, { timeout: 30000 }).then(() => true, () => false);
  const slides = () => page.evaluate(() => window.scaena.slides.slides());
  const order = async () => (await slides()).map((s) => s.id).join(" ");
  const states = async () => (await source()).split("\n").flatMap((l) => (/^state (\S+)/.exec(l) ?? []).slice(1));
  const reading = (state) => page.evaluate((s) => window.scaena.reading(s), state);
  /** Once the source differs from `was`, and the light table has its slides again. */
  const changed = async (was) => {
    await page.waitForFunction((s) => window.scaena.source() !== s, was, { timeout: 30000, polling: 50 }).catch(() => {});
    await page.evaluate(() => window.scaena.slides.settled());
    await page.waitForTimeout(300);
  };
  const undo = async (to) => {
    await page.keyboard.press("Control+z");
    await page.waitForFunction((s) => window.scaena.source() === s, to, { timeout: 30000, polling: 50 }).catch(() => {});
    await page.waitForTimeout(300);
    return (await source()) === to;
  };
  const tile = (id) => page.locator(`#slides li[data-slide="${id}"]`);

  const original = await source();
  const read = {};
  for (const s of ["intro", "revenue", "mix", "close"]) read[s] = await reading(s);
  /** Whether each state reads as it read: `as`, which state's reading each should have. */
  const readsAsItRead = async (as = {}) => {
    const off = [];
    for (const s of await states()) if ((await reading(s)) !== read[as[s] ?? s]) off.push(s);
    return off;
  };

  // Slides: in place of the preview, the slide shown selected and focused.
  await page.click("#slides-open");
  await page.waitForFunction(() => window.scaena.slides.drawn() === 3, null, { timeout: 60000 }).catch(() => {});
  check((await order()) === "intro revenue close", `three slides: ${await order()}`);
  check((await page.evaluate(() => window.scaena.slides.drawn())) === 3, "each drawn at its last state");
  check(await page.locator("#preview").isHidden(), "in place of the preview");
  check((await page.getAttribute("#slides-open", "aria-pressed")) === "true", "Slides is pressed");
  const first = await slides();
  check(first[0].selected && (await page.evaluate(() => document.activeElement?.dataset?.slide)) === "intro", "the slide shown is selected, and focused");
  const label = await tile("revenue").getAttribute("aria-label");
  check(label === "Slide 2, 2 steps", `each says its number and its steps: ${label}`);

  // Alt+← moves `close` before `revenue`: each state reads as it read.
  await tile("close").click();
  await page.keyboard.press("Alt+ArrowLeft");
  await changed(original);
  check((await order()) === "intro close revenue", `Alt+← moves close before revenue: ${await order()}`);
  check((await states()).join(" ") === "intro close revenue mix", `its states too: ${(await states()).join(" ")}`);
  check(await says("Slide 3 moved before Slide 2"), `the status says so: ${await status()}`);
  const off = await readsAsItRead();
  check(off.length === 0, `every state reads as it read: ${off.join(", ") || "all"}`);
  check((await slides()).find((s) => s.id === "close")?.selected, "close is still selected");
  check(await undo(original), "⌘Z puts it back");

  // A drag puts `close` before `intro`.
  await page.waitForFunction(() => window.scaena.slides.slides().length === 3, null, { timeout: 30000 });
  await tile("close").click();
  await tile("close").dragTo(tile("intro"), { targetPosition: { x: 8, y: 40 } });
  await changed(original);
  check((await order()) === "close intro revenue", `a drag puts close before intro: ${await order()}`);
  const dragged = await readsAsItRead();
  check(dragged.length === 0, `every state reads as it read: ${dragged.join(", ") || "all"}`);
  check(await undo(original), "⌘Z puts it back");

  // ⌘D copies `revenue` and its step just after it, the copy selected.
  await tile("revenue").click();
  await page.keyboard.press("Control+d");
  await changed(original);
  check((await order()) === "intro revenue revenue-2 close", `⌘D copies revenue after itself: ${await order()}`);
  check((await states()).join(" ") === "intro revenue mix revenue-2 mix-2 close", `its step too: ${(await states()).join(" ")}`);
  check((await slides()).filter((s) => s.selected).map((s) => s.id).join() === "revenue-2", "the copy is selected");
  const copied = await readsAsItRead({ "revenue-2": "revenue", "mix-2": "mix" });
  check(copied.length === 0, `each copy reads as what it copies, the rest as they read: ${copied.join(", ") || "all"}`);
  check(await undo(original), "⌘Z takes the copy away");

  // Delete takes `revenue` out, its step too.
  await tile("revenue").click();
  await page.keyboard.press("Delete");
  await changed(original);
  check((await order()) === "intro close", `Delete takes revenue out: ${await order()}`);
  check((await states()).join(" ") === "intro close", `its step too: ${(await states()).join(" ")}`);
  const kept = await readsAsItRead();
  check(kept.length === 0, `close reads as it read: ${kept.join(", ") || "all"}`);
  check(await undo(original), "⌘Z puts it back");

  // The slider at its foot, at its end: the slides larger, fewer to a row, each painted larger.
  const across = () =>
    page.evaluate(() => {
      const tiles = [...document.querySelectorAll("#slides li")];
      return tiles.filter((t) => t.offsetTop === tiles[0]?.offsetTop).length;
    });
  const painted = () => page.evaluate(() => document.querySelector("#slides li canvas")?.height ?? 0);
  // Once the light table holds the deck's slides again, after the undo.
  await page
    .waitForFunction(() => window.scaena.slides.slides().map((s) => s.id).join(" ") === "intro revenue close", null, { timeout: 30000 })
    .catch(() => {});
  await page.evaluate(() => window.scaena.slides.settled());
  const [wide, high] = [await across(), await painted()];
  await page.evaluate(() => {
    const size = document.querySelector("#slides input[data-size]");
    size.value = size.max;
    size.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await page.waitForFunction((h) => (document.querySelector("#slides li canvas")?.height ?? 0) > h, high, { timeout: 30000 }).catch(() => {});
  const [fewer, larger] = [await across(), await painted()];
  check(fewer < wide && larger > high, `the slider makes them larger, fewer to a row: ${wide} → ${fewer} across, painted ${high} → ${larger} high`);

  // Nothing against WCAG 2.1 AA in the light table.
  await axeInto(page);
  const found = await page.evaluate(async () => {
    const result = await window.axe.run({ include: [["#slides"]] }, { runOnly: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"] });
    return result.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
  });
  check(found.length === 0, `axe finds nothing in the light table: ${found.join("; ")}`);

  // Enter shows the slide in the canvas, its first state, and the preview back.
  await tile("close").focus();
  await page.keyboard.press("Enter");
  await page.waitForFunction(() => document.querySelector("#preview") && getComputedStyle(document.querySelector("#preview")).display !== "none", null, { timeout: 10000 }).catch(() => {});
  check(!(await page.locator("#preview").isHidden()), "Enter puts the preview back");
  await page.waitForFunction(() => window.scaena.last().states[window.scaena.shown()]?.[0] === "close", null, { timeout: 30000 }).catch(() => {});
  check((await page.evaluate(() => window.scaena.last().states[window.scaena.shown()]?.[0])) === "close", "and shows the slide's first state");

  // Escape closes it.
  await page.click("#slides-open");
  await page.waitForFunction(() => !document.querySelector("#slides").hidden, null, { timeout: 10000 });
  check(await says("Enter shows one"), `opened, it says how it works: ${await status()}`);
  // In a short window, the slider stands at the light table's foot, below every slide: none shows
  // under it or past it.
  await page.setViewportSize({ width: 1024, height: 700 });
  await page.waitForTimeout(400);
  const foot = await page.evaluate(() => {
    const sizing = document.querySelector("#slides .sizing").getBoundingClientRect();
    const table = document.querySelector("#slides").getBoundingClientRect();
    const shown = document.querySelector("#slides ol").getBoundingClientRect();
    const under = [...document.querySelectorAll("#slides li")].some((li) => {
      const r = li.getBoundingClientRect();
      const top = Math.max(r.top, shown.top);
      const bottom = Math.min(r.bottom, shown.bottom);
      return bottom > top && bottom > sizing.top + 0.5 && top < sizing.bottom - 0.5;
    });
    return { under, gap: Math.round(table.bottom - sizing.bottom) };
  });
  check(!foot.under && foot.gap <= 1, `at 1024 × 700 the slider is at the foot, over no slide: ${JSON.stringify(foot)}`);
  await page.keyboard.press("Escape");
  check(await page.locator("#slides").isHidden(), "Escape closes it");
  check(!(await status()).includes("Enter shows one"), `and what it said goes with it: ${await status()}`);
  check((await source()) === original, "the deck is as it was");

  // In that window the inspector gives way to the slide, and the palette hides it, or the
  // animations under the canvas, to give the slide their room, as a presentation app's View menu
  // does; a tab asked for shows it again.
  const area = () =>
    page.evaluate(() => {
      const r = document.querySelector("#preview").getBoundingClientRect();
      return { width: Math.round(r.width), share: (r.width * r.height) / (innerWidth * innerHeight) };
    });
  const small = await area();
  check(small.share >= 0.15, `at 1024 × 700 the slide has room: ${JSON.stringify(small)}`);
  const run = async (words) => {
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+k");
    await page.waitForFunction(() => document.querySelector("#palette").open, null, { timeout: 10000 });
    await page.keyboard.type(words);
    await page.keyboard.press("Enter");
    await page.waitForTimeout(400);
  };
  await run("Hide the inspector");
  await run("Hide the animations");
  const roomy = await area();
  check(await page.locator("#panel").isHidden(), "Hide the inspector hides it");
  check(roomy.width > small.width * 1.4, `and the slide takes their room: ${small.width} → ${roomy.width}`);
  await run("Show the inspector");
  check(!(await page.locator("#panel").isHidden()), "Show the inspector shows it again");
  await run("Show the animations");
  await page.setViewportSize({ width: 1600, height: 1000 });

  // On the trails deck, whose words have layouts of their own in its other sizes, ⌘D copies the
  // photo slide as it copies any: the copy writes each object as the slide shows it, but for its
  // layout in each size, which is the object's own.
  await page.goto(`${site.origin}/web/dist/editor.html?painter=cpu&bundle=/docs/examples/trails.deck.json`);
  await page.waitForFunction(() => window.scaena?.last()?.valid, null, { timeout: 120000 });
  await page.click("#slides-open");
  await page.waitForFunction(() => window.scaena.slides.drawn() === 15, null, { timeout: 120000 }).catch(() => {});
  const trails = await source();
  await tile("storm").click();
  await page.keyboard.press("Control+d");
  await changed(trails);
  check((await slides()).length === 16 && (await order()).includes("storm storm-2"), `⌘D copies trails' photo slide after itself: ${await order()}`);
  check(!(await status()).includes("`") && !(await status()).startsWith("Not changed"), `the status says so in a person's words: ${await status()}`);
} finally {
  await browser.close();
  await site.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "the light table moves, copies, and takes out slides, each state as it read");
process.exit(failures.length ? 1 : 0);

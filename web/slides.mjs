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
// - Enter shows a slide in the canvas at its last step, built whole, and the preview back; Escape closes it, and
//   what it said of itself in the status goes with it. Closed by Escape or the Slides button, it
//   shows the slide selected in it (editor dogfood 2).
// - The slider at its foot makes the slides larger, fewer to a row, each painted larger; in a
//   window of 1024 × 700 it stands below every slide, none showing under it or past it.
// - In that window the inspector gives way to the slide, and the palette's Hide the inspector and
//   Hide the animations give the slide their room; Show the inspector shows it again.
// - axe-core finds nothing against WCAG 2.1 AA in the light table.
// - On the trails deck, whose texts have layouts of their own in its other sizes, ⌘D copies its
//   photo slide too, and the status says so with no id.
// - A right click on a slide offers to fold it into the slide before it (editor dogfood 6), the
//   first slide offering none: on revenue, into the title's notes, which has no place for words,
//   said so before and after; on trails, the budget folded into the grants, its headline read there,
//   its claim in the grants' beat's notes, one change to undo, said in a person's words.
// Exits 1 on any failure.
import { readFile } from "node:fs/promises";
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

  // A right click on a slide offers to fold it into the slide before it (editor dogfood 6): where
  // that slide has no place for words, a title's, into its notes, said so before and after.
  await tile("revenue").click({ button: "right" });
  const offered = await page.locator(".context-menu button").allTextContents();
  check(offered.includes("Fold into slide 1's notes"), `a right click offers to fold it into the title's notes: ${offered.join(" · ")}`);
  await page.locator(".context-menu button", { hasText: "Fold into slide 1's notes" }).click();
  await changed(original);
  check(await says("Folded slide 2 into slide 1's notes"), `the status says so: ${await status()}`);
  check((await order()) === "intro close", `the revenue slide is gone: ${await order()}`);
  // The title's own notes, which it reads, take them after its own: one block under `notes """`.
  const lines = (await source()).split("\n");
  const own = lines.findIndex((l) => l.includes("Don't talk over the build."));
  const opened = lines.slice(0, own + 1).findLastIndex((l) => /^\s*notes "/.test(l));
  const closed = lines.findIndex((l, i) => i > own && /^\s*"{3,}\s*$/.test(l));
  const notes = lines.slice(opened, closed + 1).join("\n");
  // Its headline, then its notes, its step's after them.
  const theirs = /mix shifted[\s\S]*Pause on the Q3 bar[\s\S]*Same bars, stacked/.test(lines.slice(own + 1, closed).join("\n"));
  check(own >= 0 && opened >= 0 && theirs, `its words and notes are in the title's notes, after them: ${notes}`);
  check(await undo(original), "⌘Z puts it back");
  await tile("intro").click({ button: "right" });
  const atFirst = await page.locator(".context-menu button").allTextContents();
  check(!atFirst.some((t) => t.startsWith("Fold")), `the first slide offers no fold: ${atFirst.join(" · ")}`);
  await page.keyboard.press("Escape");

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

  // Enter shows the slide in the canvas at its last step, built whole as its tile draws it, and the
  // preview back (lift-up 20261110): `revenue`'s is `mix`.
  const shownId = () => page.evaluate(() => window.scaena.last().states[window.scaena.shown()]?.[0]);
  await tile("revenue").focus();
  await page.keyboard.press("Enter");
  await page.waitForFunction(() => window.scaena.last().states[window.scaena.shown()]?.[0] === "mix", null, { timeout: 30000 }).catch(() => {});
  check((await shownId()) === "mix", `Enter shows a slide at its last step: ${await shownId()}`);
  await page.click("#slides-open");
  await page.waitForFunction(() => !document.querySelector("#slides").hidden, null, { timeout: 10000 });
  await page.evaluate(() => window.scaena.slides.settled());
  // A slide of one state shows it.
  await tile("close").focus();
  await page.keyboard.press("Enter");
  await page.waitForFunction(() => document.querySelector("#preview") && getComputedStyle(document.querySelector("#preview")).display !== "none", null, { timeout: 10000 }).catch(() => {});
  check(!(await page.locator("#preview").isHidden()), "Enter puts the preview back");
  await page.waitForFunction(() => window.scaena.last().states[window.scaena.shown()]?.[0] === "close", null, { timeout: 30000 }).catch(() => {});
  check((await page.evaluate(() => window.scaena.last().states[window.scaena.shown()]?.[0])) === "close", "and a slide of one state, that state");

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

  // Closed, it shows the slide selected in it, as a presentation app's light table does (editor
  // dogfood 2): by Escape, and by the Slides button; with nothing else selected, the slide shown
  // stays.
  const shownFirst = () => page.evaluate(() => window.scaena.last().states[window.scaena.shown()]?.[0]);
  // Each at its last step: `revenue`'s is `mix` (lift-up 20261110).
  for (const [to, how, at] of [["intro", "Escape", "intro"], ["revenue", "the Slides button", "mix"]]) {
    await page.click("#slides-open");
    await page.waitForFunction(() => !document.querySelector("#slides").hidden, null, { timeout: 10000 });
    await tile(to).click();
    if (how === "Escape") await page.keyboard.press("Escape");
    else await page.click("#slides-open");
    await page.waitForFunction((s) => window.scaena.last().states[window.scaena.shown()]?.[0] === s, at, { timeout: 30000 }).catch(() => {});
    check((await page.locator("#slides").isHidden()) && (await shownFirst()) === at, `closed by ${how}, it shows the slide selected in it: ${await shownFirst()}`);
  }
  await page.click("#slides-open");
  await page.waitForFunction(() => !document.querySelector("#slides").hidden, null, { timeout: 10000 });
  await page.keyboard.press("Escape");
  check((await shownFirst()) === "mix", `with the slide shown selected, closing leaves it shown: ${await shownFirst()}`);
  check((await source()) === original, "and the deck is as it was");

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
  // The slide grows into the room they leave: it fills the stage, its width the stage's or its
  // height the stage's within 2 px, at the deck's proportions. A fixed ratio of widths sat at
  // the edge of what this window gives (1.385–1.396), whatever the chrome around the stage.
  const fill = await page.evaluate(() => {
    const desk = document.querySelector("#desk");
    const pad = getComputedStyle(desk);
    const room = [
      desk.clientWidth - parseFloat(pad.paddingLeft) - parseFloat(pad.paddingRight),
      desk.clientHeight - parseFloat(pad.paddingTop) - parseFloat(pad.paddingBottom),
    ];
    const r = document.querySelector("#preview").getBoundingClientRect();
    return { room: room.map(Math.round), slide: [Math.round(r.width), Math.round(r.height)], aspect: r.width / r.height };
  });
  const { canvas } = JSON.parse(await readFile(new URL("../docs/examples/revenue.deck.json", import.meta.url), "utf8"));
  const [width, height] = [canvas.width, canvas.height];
  const fills = Math.abs(fill.slide[0] - fill.room[0]) <= 2 || Math.abs(fill.slide[1] - fill.room[1]) <= 2;
  const proportioned = Math.abs(fill.aspect - width / height) < 0.01;
  check(
    roomy.width > small.width && fills && proportioned,
    `and the slide takes their room: ${small.width} → ${roomy.width}, filling the stage: ${JSON.stringify(fill)}`,
  );
  await run("Show Format");
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
  check(await undo(trails), "⌘Z takes the copy away");

  // Fold slide 12, the budget, into slide 11, the grants (editor dogfood 6): its headline and its
  // words after the grants' words, its beat gone and its claim in the grants' beat's notes, one
  // change to undo, said in a person's words.
  await tile("budget").click({ button: "right" });
  await page.locator(".context-menu button", { hasText: "Fold into slide 11" }).click();
  await changed(trails);
  const folded = await source();
  check(await says("Folded slide 12 into slide 11"), `the status says so: ${await status()}`);
  check((await slides()).length === 14 && !(await order()).includes("budget"), `the budget's slide is gone: ${await order()}`);
  const grants = await reading("funding");
  check(grants.includes("We came in $6,600 under budget"), `the grants' slide reads the budget's headline: ${grants}`);
  const beat = (folded.split("beat grants ")[1] ?? "").split(/\n\s*(?:beat|section) /)[0];
  check(/notes "[^\n]*under budget/.test(beat), `its claim is in the grants' beat's notes: ${beat}`);
  check(!(await status()).includes("`"), `in a person's words: ${await status()}`);
  check(await undo(trails), "⌘Z puts the slide back");
} finally {
  await browser.close();
  await site.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "the light table moves, copies, and takes out slides, each state as it read");
process.exit(failures.length ? 1 : 0);

// PLAN 2.44 check: the cue of the state shown, under the editor's preview, in headless Chromium
// (serve.mjs), the CPU painting, on the revenue example.
//
//   node web/cue.mjs     (after `just web`; from the repository's root)
//
// - `intro`'s cue: the transition, then the title's words and the subtitle's rise, each a bar.
// - The subtitle's bar, moved by a key, then dragged: its delay is the choreography item's, one
//   patch and one undo each; its end dragged sets its duration.
// - `revenue`: the transition's end, by a key, sets the state's transition; the chart's grow is
//   on a spring, so its end does not move.
// - A press on the ruler shows the cue at that time; a press on the canvas brings it back to rest;
//   Play plays the cue and comes to rest at its end, in the same state.
// - Add a motion: a fade for the backdrop as it enters `close`, and a rise for the subtitle as it
//   leaves `intro`, written in `intro`.
// - Undone, the deck is as it was, and it validates.
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
  const bars = () => page.evaluate(() => window.scaena.cue.bars());
  const bar = async (key) => (await bars()).find((b) => b.key === key);
  const trips = () => page.evaluate(() => window.scaena.trips().length);
  /** Once the status says `what`. */
  const says = (what) =>
    page.waitForFunction((w) => document.querySelector("#status").textContent.includes(w), what, { timeout: 30000 }).then(
      () => true,
      () => false,
    );
  /** Show state `id`, its cue drawn. */
  const show = async (id) => {
    const index = await page.evaluate((s) => window.scaena.opened.states.indexOf(s), id);
    const at = (await source()).indexOf(`state ${id}`) + "state ".length;
    await page.evaluate((offset) => window.scaena.cursor(offset), at);
    await page.waitForFunction((i) => window.scaena.shown() === i && window.scaena.at().index === i, index, { timeout: 30000 });
    await page.waitForFunction(() => window.scaena.cue.bars().length > 0, null, { timeout: 30000 });
    return index;
  };
  /** Once the cue is drawn again from the source the editor holds after `before` trips. */
  const redrawn = (before) =>
    page.waitForFunction((n) => window.scaena.trips().length > n, before, { timeout: 60000, polling: 50 }).then(() => page.waitForTimeout(150));
  /** One step back: Control+Z on the canvas. */
  const undo = async () => {
    const n = await trips();
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+z");
    await redrawn(n);
  };
  /** The center of the element `selector` matches. */
  const center = async (selector) => {
    const box = await page.locator(selector).boundingBox();
    return [box.x + box.width / 2, box.y + box.height / 2, box];
  };
  /** Drag from the middle of `selector` `dx` pixels across. */
  const drag = async (selector, dx) => {
    const [x, y] = await center(selector);
    await page.mouse.move(x, y);
    await page.mouse.down();
    for (let k = 1; k <= 6; k++) await page.mouse.move(x + (dx * k) / 6, y);
    await page.mouse.up();
  };
  /** How many pixels across the lanes `ms` is. */
  const pixels = async (ms) => {
    const width = (await page.locator("#cue .ruler").boundingBox()).width;
    return (ms / (await page.evaluate(() => window.scaena.cue.scale()))) * width;
  };

  const original = await source();

  // `intro`: the transition, the title's words, the subtitle's rise.
  await show("intro");
  const keys = (await bars()).map((b) => b.key);
  check(JSON.stringify(keys) === JSON.stringify(["transition", "title enter", "subtitle enter"]), `intro's cue, a bar each: ${keys}`);
  const rise = await bar("subtitle enter");
  check(rise.delay === 240 && rise.from === 240, `the subtitle's rise waits 240 ms: ${JSON.stringify(rise)}`);

  // A key moves it: 10 ms a press.
  let n = await trips();
  await page.locator('#cue [data-key="subtitle enter/delay"]').focus();
  await page.keyboard.press("ArrowRight");
  check(await says("subtitle's enter waits 250 ms"), `→ moves it 10 ms: ${await status()}`);
  await redrawn(n);
  check((await source()).includes("choreo subtitle enter:rise delay:250ms"), "written in the choreography item");
  check((await bar("subtitle enter"))?.from === 250, "and the bar is drawn there");
  await undo();
  check((await source()) === original, "one undo takes it back");

  // A drag moves it, in tens of ms.
  n = await trips();
  await drag('#cue [data-key="subtitle enter/delay"]', await pixels(300));
  check(await says("subtitle's enter waits"), `a drag moves it: ${await status()}`);
  await redrawn(n);
  const dragged = await bar("subtitle enter");
  check(Math.abs(dragged.delay - 540) <= 20 && dragged.delay % 10 === 0, `about 300 ms later, in whole tens: ${dragged.delay}`);
  await undo();
  check((await source()) === original, "one undo takes the drag back");

  // Its end dragged sets how long it lasts.
  n = await trips();
  await drag('#cue [data-key="subtitle enter/duration"]', await pixels(200));
  check(await says("subtitle's enter lasts"), `its end sets its duration: ${await status()}`);
  await redrawn(n);
  const longer = await bar("subtitle enter");
  check(Math.abs(longer.duration - 620) <= 20, `about 200 ms longer than the theme's 420: ${longer.duration}`);
  check(/choreo subtitle enter:rise delay:240ms duration:\d+ms/.test(await source()), "written on the item");
  await undo();
  check((await source()) === original, "one undo takes it back");

  // `revenue`: the transition's end sets the state's transition; the chart grows on a spring.
  await show("revenue");
  const transition = await bar("transition");
  check(transition.duration === 420, `revenue's transition, the theme's standard: ${transition.duration}`);
  n = await trips();
  await page.locator('#cue [data-key="transition/duration"]').focus();
  await page.keyboard.press("Shift+ArrowRight");
  check(await says("the transition into revenue lasts 520 ms"), `Shift+→ lengthens it 100 ms: ${await status()}`);
  await redrawn(n);
  check((await source()).includes("transition:{duration: 520ms, ease: standard}"), "written in the state's transition");
  await undo();
  check((await bar("rev enter"))?.sprung === true, "the chart's grow runs on a spring");
  check((await page.locator('#cue [data-key="rev enter/duration"]').count()) === 0, "and its end does not move");

  // A press on the ruler shows the cue at that time; a press on the canvas brings it to rest.
  const ruler = await page.locator("#cue .ruler").boundingBox();
  const spanned = await page.evaluate(() => window.scaena.last().slots.find((s) => s.state === "revenue").span);
  await page.mouse.click(ruler.x + (await pixels(300)), ruler.y + ruler.height / 2);
  await page.waitForFunction(() => window.scaena.cue.at() !== undefined && !window.scaena.canvas.busy?.(), null, { timeout: 30000 });
  const at = await page.evaluate(() => window.scaena.cue.at());
  check(Math.abs(at - 300) <= 20, `a press on the ruler shows the cue 300 ms in: ${at}`);
  check(await page.evaluate(() => document.querySelector("#preview").classList.contains("scrubbing")), "the canvas's outlines step back");
  await page.waitForFunction((t) => Math.abs(window.scaena.at().t - t) < 1, at, { timeout: 30000 }).catch(() => {});
  check(Math.abs((await page.evaluate(() => window.scaena.at().t)) - at) < 1, "and the preview is there");
  const [cx, cy] = await center("#overlay");
  await page.mouse.click(cx, cy);
  await page.waitForFunction(() => window.scaena.cue.at() === undefined, null, { timeout: 30000 }).catch(() => {});
  check((await page.evaluate(() => window.scaena.cue.at())) === undefined, "a press on the canvas brings it back to rest");
  await page.waitForFunction((s) => window.scaena.at().t >= s, spanned, { timeout: 30000 }).catch(() => {});

  // Play plays the cue, alone, and comes to rest at its end.
  const index = await page.evaluate(() => window.scaena.shown());
  await page.locator("#cue [data-play]").click();
  const played = await page
    .waitForFunction(() => window.scaena.at().playing, null, { timeout: 10000 })
    .then(
      () => true,
      () => false,
    );
  check(played, "Play plays the cue");
  await page.waitForFunction(() => !window.scaena.at().playing && window.scaena.cue.at() === undefined, null, { timeout: 30000 }).catch(() => {});
  check((await page.evaluate(() => window.scaena.at().playing)) === false, "and comes to rest at its end");
  check((await page.evaluate(() => window.scaena.at().index)) === index, "in the same state, though it holds");

  // Add a motion: the backdrop fades in as it enters `close`.
  await show("close");
  await page.evaluate(() => window.scaena.canvas.select("bg"));
  await page.waitForFunction(() => [...document.querySelectorAll("#cue [data-add] option")].some((o) => o.value.includes('"enter","bg","fade"')), null, { timeout: 30000 });
  n = await trips();
  await page.locator("#cue [data-add]").selectOption('["enter","bg","fade"]');
  check(await says("fade on bg as it enters"), `the menu adds a fade as the backdrop enters: ${await status()}`);
  await redrawn(n);
  check((await bar("bg enter")) !== undefined, "and the cue has its bar");
  await undo();

  // And the subtitle rises out as it leaves `intro`: the exit is written where it leaves from.
  await show("revenue");
  await page.waitForFunction(() => [...document.querySelectorAll("#cue [data-add] option")].some((o) => o.value.includes('"exit","subtitle","rise"')), null, { timeout: 30000 });
  n = await trips();
  await page.locator("#cue [data-add]").selectOption('["exit","subtitle","rise"]');
  check(await says("rise on subtitle as it leaves"), `a rise as the subtitle leaves: ${await status()}`);
  await redrawn(n);
  check((await bar("subtitle exit")) !== undefined, "the cue has its bar");
  const intro = (await source()).split("state revenue")[0];
  check(/subtitle[^\n]*exit:rise/.test(intro), "written in intro, which it leaves");
  await undo();
  check((await source()) === original, "undone, the deck is as it was");
  check(await page.evaluate(() => window.scaena.last().valid), "and it validates");
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

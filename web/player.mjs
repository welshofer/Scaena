// PLAN 2.2 player check: the web player's controls and its presenter view, in headless
// Chromium (serve.mjs), on the revenue example, each of whose states holds before the next
// (SPEC §2.4), and on the torture deck, whose states do not. The CPU painter paints: headless
// Chromium composites WebGPU on SwiftShader at about a frame a second, which the clock, tied
// to the display's frames, cannot be tested at.
//
//   node web/player.mjs     (after `just web`; from the repository's root)
//
// - Going on plays the next state's cue; going on again during it finishes it; going back
//   shows the state before at rest; End and Home go to the last and the first.
// - A state that holds goes on by itself when its hold is over; one that does not, and the
//   last, rests and waits.
// - The scrubber shows the deck at a time on its timeline: the frame `show` paints for the
//   same state and time.
// - A click goes on; a swipe either way goes on or back.
// - The presenter view follows the player, shows the state's notes (its beat's, where it has
//   none), and steers it.
// - With `?fps`, the frame meter reads a played cue's frames: frames a second, the worst frame,
//   late frames, and the mean paint (gate 2, criterion 1).
// Exits 1 on any failure.
import { readFile } from "node:fs/promises";
import { launch, serve } from "./serve.mjs";

const server = await serve();
const url = (bundle, view) => `${server.origin}/web/dist/?painter=cpu&bundle=${bundle}${view ? `&view=${view}` : ""}`;
const revenue = "/docs/examples/revenue.deck.json";
const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};

const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1920, height: 1200 }, deviceScaleFactor: 1 });
  const open = async (address) => {
    const page = await context.newPage();
    page.on("pageerror", (e) => failures.push(`${address}: ${e.message}`));
    page.on("console", (m) => m.type() === "error" && failures.push(`${address}: console: ${m.text()}`));
    await page.goto(address);
    await page.waitForFunction(() => window.scaena || document.querySelector("#status")?.textContent.startsWith("error"), null, {
      timeout: 120000,
    });
    const status = await page.evaluate(() => (window.scaena ? "" : document.querySelector("#status").textContent));
    if (status) throw new Error(`${address}: ${status}`);
    return page;
  };
  /** Where the player's deck is, once `test(arg)` holds of it in the page. */
  const until = (page, test, what, arg = null, timeout = 15000) =>
    page
      .waitForFunction(test, arg, { timeout, polling: 50 })
      .then(() => page.evaluate(() => window.scaena.at()))
      .catch(async () => {
        const at = await page.evaluate(() => window.scaena.at());
        failures.push(`${what}: still at ${JSON.stringify(at)}`);
        return at;
      });

  const player = await open(url(revenue));
  const slots = await player.evaluate(() => window.scaena.timeline());
  const states = slots.map((s) => s.state);
  console.log(`revenue: ${states.join(", ")}; holds ${slots.map((s) => s.hold).join(", ")} ms`);
  let at = await player.evaluate(() => window.scaena.at());
  check(at.index === 0 && !at.playing && at.t === slots[0].span, `it opens on ${states[0]}, at rest`);

  // Keys.
  await player.keyboard.press("ArrowRight");
  at = await until(player, () => window.scaena.at().index === 1, "→ goes on");
  check(at.index === 1, `→ plays ${states[1]}'s cue`);
  await player.keyboard.press("ArrowRight");
  at = await until(player, () => !window.scaena.at().playing, "→ again finishes the cue");
  check(at.index === 1 && at.t === slots[1].span && !at.playing, `→ during the cue finishes it: ${states[1]} at rest`);
  await player.keyboard.press("ArrowLeft");
  at = await until(player, () => window.scaena.at().index === 0 && !window.scaena.at().playing, "← goes back");
  check(at.index === 0 && at.t === slots[0].span, `← shows ${states[0]} at rest`);
  await player.keyboard.press("End");
  at = await until(player, (last) => window.scaena.at().index === last, "End", slots.length - 1);
  check(at.index === slots.length - 1 && !at.playing, `End shows ${states.at(-1)} at rest`);
  await player.keyboard.press("Home");
  at = await until(player, () => window.scaena.at().index === 0, "Home");
  check(at.index === 0 && !at.playing, `Home shows ${states[0]} at rest`);

  // Holds: from just before the first state's hold ends, the deck goes on by itself.
  await player.evaluate((t) => window.scaena.run(0, t), slots[0].span + slots[0].hold - 300);
  at = await until(player, () => window.scaena.at().index === 1, "the hold ends", null, 5000);
  check(at.index === 1 && at.playing, `${states[0]}'s hold of ${slots[0].hold} ms goes on to ${states[1]} by itself`);
  // The last state rests and waits.
  await player.evaluate((last) => window.scaena.run(last, 0), slots.length - 1);
  at = await until(player, () => !window.scaena.at().playing, "the last state comes to rest", null, 10000);
  check(at.index === slots.length - 1 && at.t === slots.at(-1).span, `${states.at(-1)}, the last, rests and waits`);

  // The scrubber, a step per state: halfway into the third state's step is halfway into its
  // cue, and the frame there is the one `show` paints for it.
  const half = slots[2].span / 2;
  await player.locator("#scrub").evaluate((input) => {
    input.value = "2.5";
    input.dispatchEvent(new Event("input"));
  });
  at = await until(player, () => window.scaena.at().index === 2 && !window.scaena.at().playing, "the scrubber");
  const sought = await player.locator("#stage").screenshot();
  await player.evaluate(([state, t]) => window.scaena.show(state, t), [states[2], at.t]);
  const shown = await player.locator("#stage").screenshot();
  check(at.index === 2 && Math.abs(at.t - half) <= 1, `the scrubber shows ${states[2]} ${at.t.toFixed(0)} ms into its cue`);
  check(sought.equals(shown), "the scrubber's frame is the one `show` paints for the same state and time");

  // The pointer: a click goes on; a swipe right goes back.
  await player.evaluate(() => window.scaena.seek(1));
  await player.locator("#stage").click({ position: { x: 960, y: 540 } });
  at = await until(player, () => window.scaena.at().index === 2, "a click goes on");
  check(at.index === 2, `a click goes on to ${states[2]}`);
  await player.mouse.move(800, 500);
  await player.mouse.down();
  await player.mouse.move(1400, 520, { steps: 4 });
  await player.mouse.up();
  at = await until(player, () => window.scaena.at().index === 1 && !window.scaena.at().playing, "a swipe right goes back");
  check(at.index === 1 && !at.playing, `a swipe right goes back to ${states[1]}`);

  // The presenter view follows the player, shows the notes, and steers it.
  const deck = JSON.parse(await readFile(`.${revenue}`, "utf8"));
  const beatNotes = (state) => deck.spine.sections.flatMap((s) => s.beats).find((b) => b.states?.includes(state))?.notes;
  const notes = (i) => deck.states[i].notes ?? beatNotes(states[i]) ?? "No notes.";
  const presenter = await open(url(revenue, "presenter"));
  const follows = (i) => `${i + 1} / ${states.length} · ${states[i]}`;
  const followed = await presenter
    .waitForFunction((want) => window.scaena.follows() === want, follows(1), { timeout: 15000 })
    .then(() => true, () => false);
  check(followed, `the presenter view follows the player to ${states[1]}`);
  check((await presenter.textContent("#notes")) === notes(1), `it shows ${states[1]}'s notes, its beat's`);
  check((await presenter.textContent("#nextName")) === `Next: ${states[2]}`, `it shows ${states[2]} next`);
  await presenter.click("#on");
  at = await until(player, () => window.scaena.at().index === 2, "the presenter view goes on");
  check(at.index === 2, `its ▶ steers the player on to ${states[2]}`);
  const followedOn = await presenter
    .waitForFunction((want) => window.scaena.follows() === want, follows(2), { timeout: 15000 })
    .then(() => true, () => false);
  check(followedOn && (await presenter.textContent("#notes")) === notes(2), `and follows it there, with ${states[2]}'s notes`);
  await presenter.close();

  // Fullscreen: F asks for it.
  await player.keyboard.press("f");
  const full = await player.waitForFunction(() => document.fullscreenElement !== null, null, { timeout: 5000 }).then(() => true, () => false);
  check(full, "F makes the player fullscreen");
  if (full) await player.keyboard.press("f");
  await player.close();

  // In a deck whose states do not hold, most take no time on the timeline: many stand at one
  // instant, and the player still tells them apart.
  const torture = await open(url("/tests/fixtures/torture.scaena"));
  const tSlots = await torture.evaluate(() => window.scaena.timeline());
  const instant = tSlots.filter((s) => s.start === tSlots[5].start).length;
  await torture.evaluate(() => window.scaena.seek(5));
  await torture.keyboard.press("ArrowRight");
  at = await until(torture, () => window.scaena.at().index === 6 && !window.scaena.at().playing, "→ from a state with no cue");
  check(at.index === 6, `→ from ${tSlots[5].state} goes on to ${tSlots[6].state}, one of ${instant} states at ${tSlots[5].start} ms`);
  // A state that does not hold rests at the end of its cue, and waits.
  const k = tSlots.findIndex((s, i) => i > 0 && s.span > 0 && !(s.hold > 0));
  await torture.evaluate((k) => window.scaena.run(k, 0), k);
  at = await until(torture, (k) => window.scaena.at().index === k && !window.scaena.at().playing, "a state without a hold comes to rest", k, 10000);
  check(at.index === k && at.t === tSlots[k].span, `${tSlots[k].state}, which does not hold, rests at the end of its cue and waits`);
  await torture.close();

  // The frame meter (gate 2, criterion 1): with ?fps, a cue played says how its frames went.
  const metered = await open(`${url("/tests/fixtures/torture.scaena")}&fps`);
  check(!(await metered.locator("#meter").isHidden()), "with ?fps the player shows its frame meter");
  await metered.evaluate((k) => window.scaena.run(k, 0), k);
  await until(metered, (k) => window.scaena.at().index === k && !window.scaena.at().playing, "a cue played with the meter", k, 10000);
  const reading = await metered.locator("#meter").textContent();
  const [, fps, frames] = /^([\d.]+) fps · worst \d+ ms · \d+ late of (\d+) · paint [\d.]+ ms$/.exec(reading) ?? [];
  check(Number(fps) > 0 && Number(frames) > 1, `and reads the cue's frames: ${reading}`);
  await metered.close();
  const plain = await open(url(revenue));
  check(await plain.locator("#meter").isHidden(), "without it, there is no meter");
  await plain.close();
} catch (e) {
  failures.push(String(e));
} finally {
  await browser.close();
  server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "the player goes on, back, by itself, and where it is told");
process.exit(failures.length ? 1 : 0);

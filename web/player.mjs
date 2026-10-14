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
// - The presenter's keys (PLAN 2.105): B, W, . and , turn the screen black or white and back, the
//   deck waiting under it, a hold where it stood, and a key or a click brings the slide back; a
//   slide's number then Return goes there, one the deck lacks said; L shows the laser where the
//   pointer is over the slide; ? lists the keys.
// - The presenter view follows the player, shows the state's notes (its beat's, where it has
//   none), and steers it: on, back, and with End and Home to the last and the first; and its own
//   presenter's keys steer the room's screen, its laser pointed from the slide it shows, and drawn
//   on none of the room's black screen.
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
    await page.waitForFunction(() => window.scaena || document.querySelector("#status")?.textContent.startsWith("Error"), null, {
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

  // The presenter's keys (PLAN 2.105). B: the screen black, the deck waiting where it was, a hold
  // under way standing still; a key brings the slide back, and the hold goes on from there.
  const curtain = () =>
    player.evaluate(() => {
      const c = document.querySelector(".curtain");
      return { hidden: c.hidden, color: getComputedStyle(c).backgroundColor, blank: window.scaena.blank() };
    });
  await player.evaluate((t) => window.scaena.run(0, t), slots[0].span + slots[0].hold - 700);
  await until(player, () => window.scaena.at().playing, "the first state's hold runs");
  await player.keyboard.press("b");
  await player.waitForFunction(() => window.scaena.blank() === "black" && !window.scaena.at().playing, null, { timeout: 5000 }).catch(() => {});
  let screen = await curtain();
  check(!screen.hidden && screen.color === "rgb(0, 0, 0)" && screen.blank === "black", `B turns the screen black: ${JSON.stringify(screen)}`);
  check((await player.textContent("#status")).endsWith("black screen"), `and the status says so: ${await player.textContent("#status")}`);
  await player.waitForTimeout(1500);
  at = await player.evaluate(() => window.scaena.at());
  check(at.index === 0 && !at.playing, `the deck waits under it: ${states[0]}'s hold stands still`);
  await player.keyboard.press("ArrowRight");
  screen = await curtain();
  at = await player.evaluate(() => window.scaena.at());
  check(screen.hidden && screen.blank === null && at.index === 0, "→ brings the slide back, and goes no further");
  at = await until(player, () => window.scaena.at().index === 1, "the hold goes on once the slide is back", null, 5000);
  check(at.index === 1, `and the hold goes on from where it stood, to ${states[1]}`);
  await player.evaluate(() => window.scaena.seek(1));
  // W: white; . and , turn it black and white again, W the slide back; a click on it, too.
  await player.keyboard.press("w");
  screen = await curtain();
  check(!screen.hidden && screen.color === "rgb(255, 255, 255)" && screen.blank === "white", `W turns it white: ${JSON.stringify(screen)}`);
  await player.keyboard.press(".");
  check((await curtain()).blank === "black", ". turns it black");
  await player.keyboard.press(",");
  check((await curtain()).blank === "white", ", turns it white");
  await player.keyboard.press("w");
  check((await curtain()).hidden, "W again brings the slide back");
  await player.keyboard.press("b");
  await player.mouse.click(960, 400);
  screen = await curtain();
  at = await player.evaluate(() => window.scaena.at());
  check(screen.hidden && at.index === 1, "a click brings it back, and goes no further");

  // A slide's number then Return goes there; one the deck lacks is said; Escape lets it go.
  const typed = () =>
    player.evaluate(() => {
      const t = document.querySelector(".typed");
      return t.hidden ? "" : t.textContent;
    });
  const slideAt = (n) => slots.findIndex((s) => s.said === `Slide ${n}`);
  const count = new Set(slots.map((s) => s.slide ?? s.state)).size;
  await player.keyboard.press("Home");
  await until(player, () => window.scaena.at().index === 0, "Home before a number");
  await player.keyboard.press(String(count));
  check((await typed()) === `Slide ${count} · Return goes there`, `a number typed shows: ${await typed()}`);
  await player.keyboard.press("Enter");
  at = await until(player, (k) => window.scaena.at().index === k, `slide ${count}`, slideAt(count));
  check(at.index === slideAt(count) && !(await typed()), `${count} then Return shows slide ${count}, ${states[slideAt(count)]}`);
  await player.keyboard.press("1");
  await player.keyboard.press("2");
  check((await typed()).startsWith("Slide 12 "), `digits run on: ${await typed()}`);
  await player.keyboard.press("Backspace");
  check((await typed()).startsWith("Slide 1 "), `Backspace takes the last back: ${await typed()}`);
  await player.keyboard.press("Enter");
  at = await until(player, () => window.scaena.at().index === 0, "slide 1");
  check(at.index === 0, `and Return goes to slide 1, ${states[0]}`);
  await player.keyboard.press("9");
  await player.keyboard.press("Enter");
  await player.waitForTimeout(300);
  at = await player.evaluate(() => window.scaena.at());
  check((await typed()) === `No slide 9: the deck has ${count} slides` && at.index === 0, `a slide the deck lacks is said: ${await typed()}`);
  await player.keyboard.press("2");
  await player.keyboard.press("Escape");
  await player.keyboard.press("ArrowLeft");
  at = await player.evaluate(() => window.scaena.at());
  check(!(await typed()) && at.index === 0, "Escape lets a number go");

  // L: the laser, a red dot where the pointer is over the slide.
  const laser = () =>
    player.evaluate(() => {
      const d = document.querySelector(".laser");
      const r = d.getBoundingClientRect();
      return { ...window.scaena.laser(), hidden: d.hidden, at: [r.left + r.width / 2, r.top + r.height / 2] };
    });
  await player.keyboard.press("l");
  await player.mouse.move(700, 300);
  await player.mouse.move(960, 400, { steps: 3 });
  let dot = await laser();
  const box = await player.locator("#stage").boundingBox();
  const near = (p, q) => Math.abs(p[0] - q[0]) <= 2 && Math.abs(p[1] - q[1]) <= 2;
  const fraction = [(960 - box.x) / box.width, (400 - box.y) / box.height];
  check(dot.on && !dot.hidden && near(dot.at, [960, 400]), `L shows the laser where the pointer is: ${JSON.stringify(dot)}`);
  check(
    dot.point && Math.abs(dot.point[0] - fraction[0]) < 0.01 && Math.abs(dot.point[1] - fraction[1]) < 0.01,
    `at its place on the slide: ${dot.point}`,
  );
  await player.keyboard.press("l");
  dot = await laser();
  check(!dot.on && dot.hidden, "L again puts it away");

  // ?: the keys, listed.
  const listed = () =>
    player.evaluate(() => {
      const d = document.querySelector("dialog.keylist");
      return { open: d.open, text: d.textContent };
    });
  await player.keyboard.press("?");
  const list = await listed();
  const words = ["A black screen", "A white screen", "That slide", "The laser", "Fullscreen"];
  check(list.open && words.every((w) => list.text.includes(w)), "? lists the keys");
  await player.keyboard.press("?");
  check(!(await listed()).open, "? again closes the list");
  await player.keyboard.press("?");
  await player.keyboard.press("Escape");
  check(!(await listed()).open, "and so does Escape");
  await player.evaluate(() => window.scaena.seek(1));

  // The presenter view follows the player, shows the notes, and steers it.
  const deck = JSON.parse(await readFile(`.${revenue}`, "utf8"));
  const beatNotes = (state) => deck.spine.sections.flatMap((s) => s.beats).find((b) => b.states?.includes(state))?.notes;
  const notes = (i) => deck.states[i].notes ?? beatNotes(states[i]) ?? "No notes.";
  const presenter = await open(url(revenue, "presenter"));
  // Where the player is, as a person counts its slides (PLAN 2.99): Slide 2 of 3, step 2.
  const slides = new Set(slots.map((s) => s.slide ?? s.state)).size;
  const follows = (i) => slots[i].said.replace(/^Slide (\d+)/, `Slide $1 of ${slides}`);
  const followed = await presenter
    .waitForFunction((want) => window.scaena.follows() === want, follows(1), { timeout: 15000 })
    .then(() => true, () => false);
  check(followed, `the presenter view follows the player to ${states[1]}`);
  check((await presenter.textContent("#notes")) === notes(1), `it shows ${states[1]}'s notes, its beat's`);
  check((await presenter.textContent("#nextName")) === `Next: ${slots[2].said}`, `it shows ${states[2]} next, by its slide's number: ${await presenter.textContent("#nextName")}`);
  await presenter.click("#on");
  at = await until(player, () => window.scaena.at().index === 2, "the presenter view goes on");
  check(at.index === 2, `its ▶ steers the player on to ${states[2]}`);
  const followedOn = await presenter
    .waitForFunction((want) => window.scaena.follows() === want, follows(2), { timeout: 15000 })
    .then(() => true, () => false);
  check(followedOn && (await presenter.textContent("#notes")) === notes(2), `and follows it there, with ${states[2]}'s notes`);
  // Its End and Home, as the player's own: at once, sooner than the holds would go there.
  await presenter.keyboard.press("End");
  at = await until(player, (last) => window.scaena.at().index === last, "the presenter view's End", slots.length - 1, 2000);
  check(at.index === slots.length - 1, `its End steers the player to ${states.at(-1)}`);
  await presenter.keyboard.press("Home");
  at = await until(player, () => window.scaena.at().index === 0, "the presenter view's Home", null, 2000);
  check(at.index === 0, `and its Home to ${states[0]}`);
  // Its presenter's keys steer the player's screen (PLAN 2.105): B turns the room's black, and the
  // view says so; a key brings the slide back.
  await presenter.keyboard.press("b");
  await player.waitForFunction(() => window.scaena.blank() === "black", null, { timeout: 5000 }).catch(() => {});
  const blankedSaid = await presenter
    .waitForFunction(() => window.scaena.blanked(), null, { timeout: 5000 })
    .then((h) => h.jsonValue(), () => null);
  check((await player.evaluate(() => window.scaena.blank())) === "black", "the presenter view's B turns the room's screen black");
  check(blankedSaid?.startsWith("The room sees a black screen"), `and it says so: ${blankedSaid}`);
  await presenter.keyboard.press("ArrowRight");
  await player.waitForFunction(() => window.scaena.blank() === null, null, { timeout: 5000 }).catch(() => {});
  at = await player.evaluate(() => window.scaena.at());
  check((await player.evaluate(() => window.scaena.blank())) === null && at.index === 0, "its → brings the slide back");
  const unsaid = await presenter
    .waitForFunction(() => window.scaena.blanked() === null, null, { timeout: 5000 })
    .then(() => true, () => false);
  check(unsaid, "and it says so no longer");
  // A slide's number typed there goes there, at once, though Next has the keys since its click;
  // one the deck lacks is said there, not to the room.
  check(await presenter.evaluate(() => document.activeElement?.id === "on"), "Next has the presenter view's keys");
  await presenter.keyboard.press(String(count));
  await presenter.keyboard.press("Enter");
  at = await until(player, (k) => window.scaena.at().index === k, "the presenter view's number", slideAt(count), 2000);
  check(at.index === slideAt(count), `its ${count} then Return shows slide ${count} in the player`);
  await presenter.keyboard.press("9");
  await presenter.keyboard.press("Enter");
  const there = await presenter.evaluate(() => document.querySelector(".typed").textContent);
  check(there === `No slide 9: the deck has ${count} slides` && !(await typed()), `a slide the deck lacks is said there alone: ${there}`);
  // L: the laser, pointed from the slide the presenter view shows, drawn on the room's.
  await presenter.keyboard.press("l");
  await player.waitForFunction(() => window.scaena.laser().on, null, { timeout: 5000 }).catch(() => {});
  const shownBox = await presenter.locator("#stage").boundingBox();
  await presenter.mouse.move(shownBox.x + shownBox.width / 4, shownBox.y + shownBox.height / 4);
  await presenter.mouse.move(shownBox.x + shownBox.width / 2, shownBox.y + shownBox.height / 2, { steps: 3 });
  await player.waitForFunction(() => window.scaena.laser().point, null, { timeout: 5000 }).catch(() => {});
  dot = await laser();
  const centered = (p) => p && Math.abs(p[0] - 0.5) < 0.01 && Math.abs(p[1] - 0.5) < 0.01;
  check(dot.on && !dot.hidden && centered(dot.point), `its L and its pointer put the laser on the room's slide: ${dot.point}`);
  const mirrored = await presenter
    .waitForFunction(() => window.scaena.pointed(), null, { timeout: 5000 })
    .then((h) => h.jsonValue(), () => null);
  check(centered(mirrored), `and it draws the dot where it points: ${mirrored}`);
  // A black screen takes the dot with the slide, though the presenter's pointer still points over
  // its slide, clear of what it says in the middle, and the slide back brings it back there.
  await presenter.keyboard.press("b");
  await player.waitForFunction(() => window.scaena.blank() === "black", null, { timeout: 5000 }).catch(() => {});
  await presenter.mouse.move(shownBox.x + shownBox.width / 4, shownBox.y + shownBox.height / 4, { steps: 3 });
  const quarter = (p) => p && Math.abs(p[0] - 0.25) < 0.01 && Math.abs(p[1] - 0.25) < 0.01;
  await player.waitForFunction(() => window.scaena.laser().point?.[0] < 0.3, null, { timeout: 5000 }).catch(() => {});
  dot = await laser();
  check(dot.on && quarter(dot.point) && dot.hidden, `the room's black screen shows no dot where it points: ${JSON.stringify(dot)}`);
  await presenter.keyboard.press("ArrowRight");
  await player.waitForFunction(() => window.scaena.blank() === null, null, { timeout: 5000 }).catch(() => {});
  dot = await laser();
  check(dot.on && !dot.hidden && quarter(dot.point), `and the slide back shows it where it points: ${JSON.stringify(dot)}`);
  await presenter.keyboard.press("l");
  await player.waitForFunction(() => !window.scaena.laser().on, null, { timeout: 5000 }).catch(() => {});
  check((await laser()).hidden, "its L again puts the laser away");
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

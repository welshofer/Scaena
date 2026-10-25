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
// - The window is the deck's, by its title, and the bar's buttons say what they do and their keys.
//   The slide stands in the middle of the room the bar leaves, and fullscreen, of the screen, on
//   black. At 1024 × 768, and at 700 × 400, where the bar takes two lines, every control of the
//   bar is in the window, the slide over it.
// - The presenter's keys (PLAN 2.105): B, W, . and , turn the screen black or white and back, the
//   deck waiting under it, a hold where it stood, and a key or a click brings the slide back; a
//   slide's number then Return goes there, one the deck lacks said, 0 among them; L shows the laser
//   where the pointer is over the slide; ? lists the keys.
// - The end of the show (PLAN 2.120): going on past the last state turns the screen black and says
//   the show has ended, the status too; ← shows the last slide again, going on again keeps it,
//   and a click or Escape leaves it, back on the last slide, and in fullscreen out of it.
// - A format the deck does not lay out in, and a promise no handler caught, are said in the
//   player's status, the slide shown staying (lift-up STAB-5).
// - The presenter view's timer waits at 0:00 until the deck goes on, and a presenter view the
//   browser blocks is said in the player's status (lift-up USE-2).
// - The presenter view follows the player, shows the state's notes (its beat's, where it has
//   none), and steers it: on, back, and with End and Home to the last and the first; and its own
//   presenter's keys steer the room's screen, its laser pointed from the slide it shows, and drawn
//   on none of the room's black screen. It shows the next state in the format the player shows,
//   and at the last state nothing as next, and its → past it ends the room's show, which it says
//   over its slide. Its timer pauses, runs on, and resets to 0:00, and the time of day stands
//   beside it (PLAN 2.121); after a press on Resume, Space goes on and presses it no more. P again
//   brings it to the front as it is, its timer running. Its notes scroll, long or short, the next
//   state's from their top, A+, A−, +, and - grow and shrink them, the size kept for the next view
//   (lift-up USE-4), and its buttons and its timer's stay in its window. With no player
//   open, or once the player closes, even as the view opens, it says so, every word of it, and
//   its buttons wait until one is.
// - A link at rest shows a hand under the pointer.
// - With `?fps`, the frame meter reads a played cue's frames: frames a second, the worst frame,
//   late frames, and the mean paint (gate 2, criterion 1).
// - The screen stays awake while the deck is presented (lift-up REL-1): fullscreen asks for a
//   screen wake lock and leaving it lets it go, one the browser let go while the page was hidden
//   is asked for again as the page shows, and the presenter view holds one while it is open. The
//   wake lock API is stood in for, headless Chromium having no screen to keep awake.
// - Fullscreen, the pointer over the slide hides once it has rested two seconds, from the moment
//   the show fills the screen, and shows again as it moves (lift-up ATTR-4).
// - At 2560 × 1440 and twice the density (lift-up ATTR-2), the slide fills the room the bar
//   leaves, and in fullscreen the screen, painted at the size it is shown in device pixels by
//   WebGPU, and by the CPU painter at the canvas's own size, scaled up.
// Exits 1 on any failure.
import { mkdir, readFile, symlink, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
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
  // The screen wake lock API, stood in for: each lock asked for is counted, and `held` is how many
  // are held now; `drop()` lets them go as the browser does when the page is hidden.
  await context.addInitScript(() => {
    const locks = new Set();
    const stand = { asked: 0, get held() { return locks.size; }, drop() { for (const l of locks) l.release(); } };
    Object.defineProperty(navigator, "wakeLock", {
      value: {
        request: async (type) => {
          if (type !== "screen") throw new TypeError(type);
          stand.asked += 1;
          const lock = new EventTarget();
          lock.type = type;
          lock.released = false;
          lock.release = async () => {
            if (lock.released) return;
            lock.released = true;
            locks.delete(lock);
            lock.dispatchEvent(new Event("release"));
          };
          locks.add(lock);
          return lock;
        },
      },
    });
    window.wake = stand;
  });
  const open = async (address) => {
    const page = await context.newPage();
    // The one promise left uncaught on purpose, to be said in the status (lift-up STAB-5).
    const meant = (text) => text.includes("a promise no one caught");
    page.on("pageerror", (e) => meant(e.message) || failures.push(`${address}: ${e.message}`));
    page.on("console", (m) => m.type() === "error" && !meant(m.text()) && failures.push(`${address}: console: ${m.text()}`));
    await page.goto(address);
    await page.waitForFunction(() => window.scaena || document.querySelector("#status")?.textContent.startsWith("Error"), null, {
      timeout: 120000,
    });
    const status = await page.evaluate(() => (window.scaena ? "" : document.querySelector("#status").textContent));
    if (status) throw new Error(`${address}: ${status}`);
    return page;
  };
  /** How many screen wake locks `page` asked for, and holds now (lift-up REL-1). */
  const wake = (page) => page.evaluate(() => ({ asked: window.wake.asked, held: window.wake.held }));

  // A deck whose theme is written in it, as deck.schema.json allows, opens from its URL as from a
  // bundle (lift-up FUNC-5): the revenue example with its theme inline, served beside it.
  {
    const deck = JSON.parse(await readFile("docs/examples/revenue.deck.json", "utf8"));
    deck.theme = JSON.parse(await readFile(new URL(deck.theme, new URL("../docs/examples/", import.meta.url)), "utf8"));
    const served = (u) => u.pathname === "/docs/examples/inline-theme.deck.json";
    await context.route(served, (r) => r.fulfill({ contentType: "application/json", body: JSON.stringify(deck) }));
    const inline = await open(url("/docs/examples/inline-theme.deck.json")).then(
      (page) => page,
      (e) => String(e),
    );
    const at = typeof inline === "string" ? inline : await inline.evaluate(() => window.scaena.at());
    check(typeof inline !== "string" && at.index === 0, `a deck with its theme inline opens from its URL: ${JSON.stringify(at)}`);
    if (typeof inline !== "string") await inline.close();
  }
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
  const { meta } = JSON.parse(await readFile(`.${revenue}`, "utf8"));
  // The window, and the bar's buttons.
  check((await player.title()) === meta.title, `the player's window is the deck's, by its title: ${await player.title()}`);
  const named = await player.evaluate(() =>
    ["back", "on", "full", "present"].map((id) => [id, document.getElementById(id).getAttribute("aria-label"), document.getElementById(id).title]),
  );
  const next = await player.getByRole("button", { name: "Next", exact: true }).getAttribute("id", { timeout: 2000 }).catch(() => null);
  check(next === "on" && named.every(([, , title]) => title), `its buttons say what they do, and their keys, ▶ as Next: ${JSON.stringify(named)}`);
  /** Where the slide stands, and the room the bar at the window's foot leaves it: the window's
   * height in fullscreen. */
  const standing = (page) =>
    page.evaluate(() => {
      const r = document.querySelector("#stage").getBoundingClientRect();
      const bar = document.querySelector(".bar");
      const room = bar.offsetParent === null ? innerHeight : innerHeight - bar.offsetHeight;
      const foot = bar.offsetParent === null || Math.abs(bar.getBoundingClientRect().bottom - innerHeight) <= 1;
      return { top: r.top, height: r.height, room, foot, background: getComputedStyle(document.body).backgroundColor };
    });
  const stands = await standing(player);
  check(
    stands.foot && Math.abs(stands.top - (stands.room - stands.height) / 2) <= 1,
    `the slide stands in the middle of the room the bar at the window's foot leaves: ${JSON.stringify(stands)}`,
  );
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
  await player.keyboard.press("0");
  await player.keyboard.press("Enter");
  await player.waitForTimeout(300);
  at = await player.evaluate(() => window.scaena.at());
  check((await typed()) === `No slide 0: the deck has ${count} slides` && at.index === 0, `and so is 0, which goes nowhere: ${await typed()}`);
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

  // The end of the show (PLAN 2.120): → past the last state turns the screen black and says the
  // show has ended; ← shows the last slide again; → again keeps it; a click or Escape leaves it.
  const ending = () =>
    player.evaluate(() => {
      const end = document.querySelector(".end");
      return {
        shown: !end.hidden,
        color: getComputedStyle(end).backgroundColor,
        words: end.textContent,
        ended: window.scaena.ended(),
        index: window.scaena.at().index,
        status: document.querySelector("#status").textContent,
      };
    });
  const last = slots.length - 1;
  await player.keyboard.press("End");
  await until(player, (last) => window.scaena.at().index === last && !window.scaena.at().playing, "End before the end of the show", last);
  let end = await ending();
  check(!end.shown && !end.ended, "the last state shows no end of the show");
  await player.keyboard.press("ArrowRight");
  end = await ending();
  check(
    end.shown && end.ended && end.color === "rgb(0, 0, 0)" && end.words.startsWith("The show has ended.") && end.index === last,
    `→ past the last state shows the end of the show, the deck on the last under it: ${JSON.stringify(end)}`,
  );
  check(end.status.endsWith("end of the show"), `and the status says so: ${end.status}`);
  await player.keyboard.press("ArrowLeft");
  end = await ending();
  check(!end.shown && !end.ended && end.index === last, `← shows the last slide again: ${JSON.stringify(end)}`);
  await player.keyboard.press("ArrowRight");
  check((await ending()).ended, "→ ends the show again");
  for (const key of ["ArrowRight", " ", "PageDown"]) await player.keyboard.press(key);
  end = await ending();
  check(end.shown && end.ended && end.index === last, `and going on again keeps it, by →, Space, and PageDown: ${JSON.stringify(end)}`);
  await player.mouse.click(960, 400);
  end = await ending();
  check(!end.ended && end.index === last, "a click on the end of the show leaves it");
  await player.keyboard.press("ArrowRight");
  await player.keyboard.press("Escape");
  end = await ending();
  check(!end.ended && end.index === last, "and so does Escape");
  await player.evaluate(() => window.scaena.seek(1));

  // The presenter view follows the player, shows the notes, and steers it.
  const deck = JSON.parse(await readFile(`.${revenue}`, "utf8"));
  const beatNotes = (state) => deck.spine.sections.flatMap((s) => s.beats).find((b) => b.states?.includes(state))?.notes;
  const notes = (i) => deck.states[i].notes ?? beatNotes(states[i]) ?? "No notes.";
  const presenter = await open(url(revenue, "presenter"));
  check((await presenter.title()) === `Presenter view · ${meta.title}`, `its window is the deck's, marked as the presenter's: ${await presenter.title()}`);
  // Where the player is, as a person counts its slides (PLAN 2.99): Slide 2 of 3, step 2.
  const slides = new Set(slots.map((s) => s.slide ?? s.state)).size;
  const follows = (i) => slots[i].said.replace(/^Slide (\d+)/, `Slide $1 of ${slides}`);
  const followed = await presenter
    .waitForFunction((want) => window.scaena.follows() === want, follows(1), { timeout: 15000 })
    .then(() => true, () => false);
  check(followed, `the presenter view follows the player to ${states[1]}`);
  check((await wake(presenter)).held === 1, `the presenter view holds a screen wake lock while it is open: ${JSON.stringify(await wake(presenter))}`);
  check((await presenter.textContent("#notes")) === notes(1), `it shows ${states[1]}'s notes, its beat's`);
  check((await presenter.textContent("#nextName")) === `Next: ${slots[2].said}`, `it shows ${states[2]} next, by its slide's number: ${await presenter.textContent("#nextName")}`);
  // It speaks to a screen reader as the player does (lift-up USE-1): where the deck is, and how
  // the state up reads, each in a polite live region; the timer a timer, and the notes named.
  const said = await presenter.evaluate(() => ({
    where: document.querySelector("#where").getAttribute("aria-live"),
    reading: document.querySelector("#reading")?.getAttribute("aria-live"),
    clock: document.querySelector("#clock").getAttribute("role"),
    notes: document.querySelector("#notes").getAttribute("aria-label"),
  }));
  check(
    said.where === "polite" && said.reading === "polite" && said.clock === "timer" && said.notes === "Speaker notes",
    `the presenter view's place and reading are live, its clock a timer, its notes named: ${JSON.stringify(said)}`,
  );
  const readsAsPlayer = await presenter
    .waitForFunction((html) => document.querySelector("#reading").innerHTML === html && html !== "", await player.innerHTML("#reading"), { timeout: 15000 })
    .then(() => true, () => false);
  check(readsAsPlayer, `the presenter view reads ${states[1]} as the player reads it`);
  // The notes' size (lift-up USE-4): 20 px to start, A+ and + a step up, A− and - a step down,
  // kept in this browser for the next presenter view; the player answers neither key.
  const notesPx = (page) => page.evaluate(() => parseFloat(getComputedStyle(document.querySelector("#notes")).fontSize));
  const startSize = await notesPx(presenter);
  await presenter.click("#larger");
  const clicked = await notesPx(presenter);
  await presenter.keyboard.press("+");
  const keyed = await notesPx(presenter);
  await presenter.keyboard.press("-");
  const back = await notesPx(presenter);
  check(startSize === 20 && clicked === 24 && keyed === 28 && back === 24, `A+ and + make the notes larger, and - smaller: ${startSize}, ${clicked}, ${keyed}, then ${back} px`);
  const atPlayer = (await player.evaluate(() => window.scaena.at())).index;
  await player.keyboard.press("+");
  check((await player.evaluate(() => window.scaena.at())).index === atPlayer, "the player itself answers no notes key");
  // Its timer (PLAN 2.121): Pause stops it where it stands, Reset puts it back to 0:00, paused as it
  // was, and Resume runs it on from there; the time of day stands beside it.
  const timer = () => presenter.evaluate(() => window.scaena.timer());
  const secondsOf = (shows) => shows.split(":").reduce((sum, part) => sum * 60 + Number(part), 0);
  await presenter.waitForFunction(() => window.scaena.timer().shows !== "0:00", null, { timeout: 5000 }).catch(() => {});
  await presenter.click("#pause");
  const stopped = await timer();
  await presenter.waitForTimeout(1500);
  const stood = await timer();
  check(
    stopped.paused && secondsOf(stopped.shows) >= 1 && stood.shows === stopped.shows && (await presenter.textContent("#pause")) === "Resume",
    `Pause stops the presenter view's timer where it stands, and offers Resume: ${stopped.shows}, then ${stood.shows}`,
  );
  await presenter.click("#reset");
  const reset = await timer();
  check(reset.shows === "0:00" && reset.paused, `Reset puts it back to 0:00, paused as it was: ${JSON.stringify(reset)}`);
  await presenter.waitForTimeout(1200);
  check((await timer()).shows === "0:00", "and it waits there");
  await presenter.click("#pause");
  await presenter.waitForFunction(() => window.scaena.timer().shows !== "0:00", null, { timeout: 5000 }).catch(() => {});
  const resumed = await timer();
  check(!resumed.paused && secondsOf(resumed.shows) >= 1 && secondsOf(resumed.shows) <= 3, `Resume runs it on from 0:00: ${resumed.shows}`);
  // A press on its buttons leaves the keys where they were: Space then goes on, and presses
  // Resume no more. ← brings the player back.
  await presenter.keyboard.press(" ");
  at = await until(player, () => window.scaena.at().index === 2, "Space after a press on Resume");
  const going = await timer();
  check(at.index === 2 && !going.paused, `Space after a press on Resume goes on, and the timer runs on: ${JSON.stringify(going)}`);
  await presenter.keyboard.press("ArrowLeft");
  await until(player, () => window.scaena.at().index === 1 && !window.scaena.at().playing, "← after Space");
  // The time of day, as this machine's clock reads it: this minute, or the one before it where the
  // minute turned between the view's tick and this read.
  const day = await presenter.evaluate(() => {
    const read = (ms) => new Date(ms).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
    return { shown: window.scaena.timer().now, now: read(Date.now()), before: read(Date.now() - 1000) };
  });
  check(day.shown === day.now || day.shown === day.before, `the time of day stands beside it: ${JSON.stringify(day)}`);
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
  /** What the presenter view shows as next, once it follows the player to state `i`. */
  const nextShown = async (i) => {
    await presenter.waitForFunction((want) => window.scaena.follows() === want, follows(i), { timeout: 15000 }).catch(() => {});
    return presenter.evaluate(() => {
      const shown = document.querySelector("#upnext");
      return { name: document.querySelector("#nextName").textContent, shown: !shown.hidden && getComputedStyle(shown).display !== "none" };
    });
  };
  const atEnd = await nextShown(slots.length - 1);
  check(atEnd.name === "The end of the deck." && !atEnd.shown, `at the last state it says the deck ends there, and shows no slide as next: ${JSON.stringify(atEnd)}`);
  // Its → past the last ends the room's show, and it says so over its slide; its ← brings the last
  // slide back (PLAN 2.120).
  await presenter.keyboard.press("ArrowRight");
  const endedThere = await player.waitForFunction(() => window.scaena.ended(), null, { timeout: 5000 }).then(() => true, () => false);
  const endSaid = await presenter
    .waitForFunction(() => window.scaena.blanked(), null, { timeout: 5000 })
    .then((h) => h.jsonValue(), () => null);
  check(endedThere && endSaid?.startsWith("The room sees the end of the show"), `its → past the last ends the room's show, and it says so: ${endSaid}`);
  await presenter.keyboard.press("ArrowLeft");
  const backThere = await player.waitForFunction(() => !window.scaena.ended(), null, { timeout: 5000 }).then(() => true, () => false);
  const unsaidEnd = await presenter.waitForFunction(() => window.scaena.blanked() === null, null, { timeout: 5000 }).then(() => true, () => false);
  at = await player.evaluate(() => window.scaena.at());
  check(backThere && unsaidEnd && at.index === slots.length - 1, "and its ← shows the room the last slide again, and says so no longer");
  await presenter.keyboard.press("Home");
  at = await until(player, () => window.scaena.at().index === 0, "the presenter view's Home", null, 2000);
  check(at.index === 0, `and its Home to ${states[0]}`);
  const fromStart = await nextShown(0);
  check(fromStart.name === `Next: ${slots[1].said}` && fromStart.shown, `back at the first, it shows the next again: ${JSON.stringify(fromStart)}`);
  // The player turned to another format, the view paints the next state in it, as it paints the
  // slide; and again at the deck's own size.
  const turnTo = async (format) => {
    await player.evaluate((format) => {
      const picker = document.querySelector("#format");
      picker.value = format;
      picker.dispatchEvent(new Event("change"));
    }, format);
    // Shown there once its status says where the deck is again, in 9:16 or no longer in it: an error
    // the status still says is not the format shown.
    await player
      .waitForFunction(
        (format) => {
          const said = document.querySelector("#status").textContent;
          return !said.startsWith("Error:") && said.includes("9:16") === (format === "9:16");
        },
        format,
        { timeout: 15000 },
      )
      .catch(() => failures.push(`the player turned to ${format || "its own size"}`));
  };
  const upright = (want) =>
    presenter
      .waitForFunction((want) => (window.scaena.coming()?.[1] > window.scaena.coming()?.[0]) === want, want, { timeout: 15000 })
      .then(() => true, () => false);
  await turnTo("9:16");
  const tall = await upright(true);
  await turnTo("");
  const wide = await upright(false);
  check(tall && wide, `it paints the next state in the format the player shows, 9:16 and its own: ${tall}, ${wide}`);
  // A format the deck does not lay out in, and a promise no handler caught (lift-up STAB-5): each is
  // said in the player's status, the slide shown staying, and the player goes on.
  const before = await player.evaluate(() => window.scaena.at().index);
  await player.evaluate(() => {
    const picker = document.querySelector("#format");
    picker.add(new Option("1:1", "1:1"));
    picker.value = "1:1";
    picker.dispatchEvent(new Event("change"));
  });
  const refused = await player
    .waitForFunction(() => document.querySelector("#status").textContent.startsWith("Error:"), null, { timeout: 15000 })
    .then(() => player.evaluate(() => document.querySelector("#status").textContent), () => null);
  check(refused?.includes("1:1") && (await player.evaluate(() => window.scaena.at().index)) === before, `a format the deck does not lay out in is said, the slide staying: ${refused}`);
  await player.evaluate(() => {
    const picker = document.querySelector("#format");
    picker.remove(picker.options.length - 1);
    void Promise.reject(new Error("a promise no one caught"));
  });
  const lost = await player
    .waitForFunction(() => document.querySelector("#status").textContent === "Error: a promise no one caught", null, { timeout: 5000 })
    .then(() => true, () => false);
  check(lost, "a promise no handler caught is said in the status");
  await turnTo("");
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
  await presenter.keyboard.press("0");
  await presenter.keyboard.press("Enter");
  await player.waitForTimeout(300);
  const zero = await presenter.evaluate(() => document.querySelector(".typed").textContent);
  at = await player.evaluate(() => window.scaena.at());
  check(zero === `No slide 0: the deck has ${count} slides` && at.index === slideAt(count), `and 0 too, the player going nowhere: ${zero}`);
  // N opens every slide, drawn small, the one the room sees marked (lift-up 10-24b USE-1); Home and
  // Return go to the first, through the player, and End and Return back to the last.
  await presenter.keyboard.press("n");
  const navigated = await presenter
    .waitForFunction((n) => { const v = window.scaena.navigator(); return v && v.drawn === n && v; }, count, { timeout: 10000 })
    .then((h) => h.jsonValue(), async () => presenter.evaluate(() => window.scaena.navigator()));
  check(
    navigated?.slides.length === count && navigated.drawn === count && navigated.marked === count - 1,
    `N shows every slide, drawn, the one the room sees marked: ${JSON.stringify(navigated)}`,
  );
  await presenter.keyboard.press("Home");
  await presenter.keyboard.press("Enter");
  at = await until(player, (k) => window.scaena.at().index === k, "the navigator's first slide", 0, 2000);
  const putAway = (await presenter.evaluate(() => window.scaena.navigator())) === null;
  check(at.index === 0 && putAway, "Home then Return goes to the first slide in the player, and closes it");
  await presenter.click("#navigate");
  await presenter.waitForFunction(() => window.scaena.navigator(), null, { timeout: 5000 }).catch(() => {});
  await presenter.keyboard.press("End");
  await presenter.keyboard.press("Enter");
  at = await until(player, (k) => window.scaena.at().index === k, "the navigator's last slide", slideAt(count), 2000);
  check(at.index === slideAt(count), `its Slides button opens it too, and End then Return goes to slide ${count}`);
  // L: the laser, pointed from the slide the presenter view shows, drawn on the room's.
  await presenter.keyboard.press("l");
  await player.waitForFunction(() => window.scaena.laser().on, null, { timeout: 5000 }).catch(() => {});
  const shownBox = await presenter.locator("#stage").boundingBox();
  await presenter.mouse.move(shownBox.x + shownBox.width / 4, shownBox.y + shownBox.height / 4);
  await presenter.mouse.move(shownBox.x + shownBox.width / 2, shownBox.y + shownBox.height / 2, { steps: 3 });
  // The move's three steps are three points, each its own message: wait for the last.
  await player
    .waitForFunction(() => { const p = window.scaena.laser().point; return !!p && Math.abs(p[0] - 0.5) < 0.01 && Math.abs(p[1] - 0.5) < 0.01; }, null, { timeout: 5000 })
    .catch(() => {});
  dot = await laser();
  const centered = (p) => p && Math.abs(p[0] - 0.5) < 0.01 && Math.abs(p[1] - 0.5) < 0.01;
  check(dot.on && !dot.hidden && centered(dot.point), `its L and its pointer put the laser on the room's slide: ${dot.point}`);
  await presenter
    .waitForFunction(() => { const p = window.scaena.pointed(); return !!p && Math.abs(p[0] - 0.5) < 0.01 && Math.abs(p[1] - 0.5) < 0.01; }, null, { timeout: 5000 })
    .catch(() => {});
  const mirrored = await presenter.evaluate(() => window.scaena.pointed());
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

  // P opens the presenter view in a window of its own; P again, or Presenter, brings it to the
  // front as it is, its timer running, and loads nothing anew.
  const [popup] = await Promise.all([context.waitForEvent("page"), player.keyboard.press("p")]);
  popup.on("pageerror", (e) => failures.push(`the presenter view's window: ${e.message}`));
  const seconds = (least = 0) => {
    const [m, s] = document.querySelector("#clock").textContent.split(":").map(Number);
    return m * 60 + s >= least && m * 60 + s;
  };
  await popup.waitForFunction(() => window.scaena?.follows?.(), null, { timeout: 120000 });
  check((await notesPx(popup)) === 24, `a presenter view opened again keeps the notes' size: ${await notesPx(popup)} px`);
  await popup.waitForFunction(seconds, 2, { timeout: 15000, polling: 100 }).catch(() => {});
  let loads = 0;
  popup.on("framenavigated", (frame) => frame === popup.mainFrame() && loads++);
  const pages = context.pages().length;
  const ran = await popup.evaluate(seconds);
  await player.bringToFront();
  await player.keyboard.press("p");
  await player.click("#present");
  await player.waitForTimeout(1200);
  const runs = await popup.evaluate(seconds);
  check(loads === 0 && runs >= ran && ran >= 2 && context.pages().length === pages, `P again, and Presenter, bring the presenter view to the front as it is, its timer running: ${ran} s, then ${runs} s, ${loads} loaded anew`);
  await popup.close();

  // Fullscreen: F asks for it, and the slide stands in the middle of the screen, on black.
  check((await wake(player)).held === 0, "before fullscreen, the player holds no wake lock");
  await player.keyboard.press("f");
  const full = await player.waitForFunction(() => document.fullscreenElement !== null, null, { timeout: 5000 }).then(() => true, () => false);
  check(full, "F makes the player fullscreen");
  if (full) {
    // The screen stays awake (lift-up REL-1): a lock held, and one the browser let go while the
    // page was hidden asked for again as the page shows.
    const locked = await player.waitForFunction(() => window.wake.held === 1, null, { timeout: 5000 }).then(() => true, () => false);
    check(locked, `fullscreen, the player holds a screen wake lock: ${JSON.stringify(await wake(player))}`);
    const before = (await wake(player)).asked;
    await player.evaluate(() => {
      window.wake.drop();
      document.dispatchEvent(new Event("visibilitychange"));
    });
    const again = await player.waitForFunction((n) => window.wake.asked === n + 1 && window.wake.held === 1, before, { timeout: 5000 }).then(() => true, () => false);
    check(again, `let go while the page was hidden, the lock is asked for again as it shows: ${JSON.stringify(await wake(player))}`);
    await player.waitForFunction(() => document.querySelector(".bar").offsetParent === null, null, { timeout: 5000 }).catch(() => {});
    const centred = await standing(player);
    check(
      Math.abs(centred.top - (centred.room - centred.height) / 2) <= 1 && centred.background === "rgb(0, 0, 0)",
      `in the middle of the screen, on black: ${JSON.stringify(centred)}`,
    );
    // The pointer over the slide hides once it has rested two seconds, and shows as it moves
    // (lift-up ATTR-4).
    const cursor = () => player.evaluate(() => getComputedStyle(document.querySelector("#stage")).cursor);
    await player.waitForTimeout(2300);
    const entered = await cursor();
    await player.mouse.move(400, 300);
    await player.mouse.move(420, 310);
    const moving = await cursor();
    await player.waitForTimeout(2300);
    const rested = await cursor();
    await player.mouse.move(440, 320);
    const moved = await cursor();
    check(
      entered === "none" && moving !== "none" && rested === "none" && moved !== "none",
      `fullscreen, the pointer hides from the start and over the slide at rest, and shows as it moves: ${entered}, ${moving}, ${rested}, ${moved}`,
    );
    // At the end of the show, going on again keeps it, fullscreen; a click leaves fullscreen, on
    // the last slide (PLAN 2.120).
    await player.keyboard.press("End");
    await until(player, (last) => window.scaena.at().index === last && !window.scaena.at().playing, "End in fullscreen", slots.length - 1);
    await player.keyboard.press("ArrowRight");
    const endedFull = await player.evaluate(() => window.scaena.ended() && document.fullscreenElement !== null);
    await player.keyboard.press("ArrowRight");
    const keptFull = await player.evaluate(() => window.scaena.ended() && document.fullscreenElement !== null);
    await player.mouse.click(400, 300);
    const left = await player.waitForFunction(() => document.fullscreenElement === null, null, { timeout: 5000 }).then(() => true, () => false);
    at = await player.evaluate(() => window.scaena.at());
    check(
      endedFull && keptFull && left && !(await player.evaluate(() => window.scaena.ended())) && at.index === slots.length - 1,
      "in fullscreen, → past the last ends the show, → again keeps it, and a click leaves fullscreen, on the last slide",
    );
    if (!left) await player.keyboard.press("f");
    const slept = await player.waitForFunction(() => window.wake.held === 0, null, { timeout: 5000 }).then(() => true, () => false);
    check(slept, `out of fullscreen, the lock is let go: ${JSON.stringify(await wake(player))}`);
  }
  await player.close();

  // The presenter view with no player to follow says so, and how to open one; its buttons wait.
  const alone = await open(url(revenue, "presenter"));
  const told = await alone
    .waitForFunction(() => document.querySelector("#status").textContent.startsWith("The player is not open."), null, { timeout: 10000 })
    .then(() => true, () => false);
  const lone = await alone.evaluate(() => ({
    status: document.querySelector("#status").textContent,
    link: document.querySelector("#status a")?.href ?? "",
    waiting: document.querySelector("#on").disabled && document.querySelector("#back").disabled,
  }));
  const linked = lone.link && !new URL(lone.link).searchParams.has("view") && new URL(lone.link).searchParams.get("bundle") === revenue;
  check(told && lone.waiting && linked, `the presenter view alone says the player is not open, and links to it: ${JSON.stringify(lone)}`);
  // Every word of it, in a window 1024 wide too, where it takes two lines.
  await alone.setViewportSize({ width: 1024, height: 640 });
  const whole = await alone.evaluate(() => {
    const said = document.querySelector("#status");
    return [said.scrollWidth, said.clientWidth];
  });
  check(whole[0] <= whole[1], `and says every word of it in a window 1024 wide: ${whole[0]} px of words in ${whole[1]} px`);
  await alone.close();

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
  // A link at rest shows a hand under the pointer, as a click there follows it (PLAN 2.70).
  await torture.evaluate((k) => window.scaena.seek(k), tSlots.findIndex((s) => s.state === "links"));
  const spot = await torture.evaluate(async () => {
    for (let y = 0.05; y < 0.95; y += 0.025) for (let x = 0.05; x < 0.95; x += 0.025) if (await window.scaena.linkAt([x, y])) return [x, y];
    return null;
  });
  const tBox = await torture.locator("#stage").boundingBox();
  const hand = (want) =>
    torture
      .waitForFunction((want) => (getComputedStyle(document.querySelector("#stage")).cursor === "pointer") === want, want, { timeout: 5000 })
      .then(() => true, () => false);
  if (spot) await torture.mouse.move(tBox.x + spot[0] * tBox.width, tBox.y + spot[1] * tBox.height, { steps: 3 });
  check(Boolean(spot) && (await hand(true)), `over a link on the links slide, the pointer is a hand: at ${spot}`);
  await torture.mouse.move(tBox.x + 4, tBox.y + tBox.height - 4, { steps: 3 });
  check(await hand(false), "and off it, it is not");
  await torture.close();

  // A long deck in a window a projector's size, 1024 × 768, and in a small one, 700 × 400, where
  // the bar takes two lines: every control of the bar in the window, the status said, the slide
  // over the bar, and the page neither wider nor taller than the window.
  const higher = "/docs/examples/higher-ed.deck.json";
  const long = await open(`${url(higher)}&state=curriculum`);
  for (const [width, height] of [[1024, 768], [700, 400]]) {
    await long.setViewportSize({ width, height });
    const fits = await long.evaluate(() => {
      const box = (e) => e.getBoundingClientRect();
      return {
        scroll: [document.documentElement.scrollWidth, document.documentElement.scrollHeight],
        controls: [...document.querySelectorAll(".bar > *")].filter((e) => !e.hidden).map((e) => [e.id, Math.round(box(e).right), Math.round(box(e).bottom)]),
        status: box(document.querySelector("#status")).width,
        slide: Math.round(box(document.querySelector("#stage")).bottom),
        bar: Math.round(box(document.querySelector(".bar")).top),
      };
    });
    check(
      fits.scroll[0] <= width &&
        fits.scroll[1] <= height &&
        fits.controls.every(([, right, bottom]) => right <= width && bottom <= height) &&
        fits.status > 0 &&
        fits.slide <= fits.bar,
      `at ${width} × ${height} the bar of a deck with long claims fits the window, under the slide: ${JSON.stringify(fits)}`,
    );
  }
  await long.setViewportSize({ width: 1024, height: 768 });
  // Its presenter view, on a slide with long notes: they scroll, and Back and Next stay in sight.
  const longView = await open(url(higher, "presenter"));
  await longView
    .waitForFunction(() => window.scaena.follows() && document.querySelector("#notes").textContent, null, { timeout: 30000 })
    .catch(() => {});
  for (const [width, height] of [[1280, 800], [1024, 640]]) {
    await longView.setViewportSize({ width, height });
    const seen = await longView.evaluate(() => {
      const bottom = (id) => Math.round(document.getElementById(id).getBoundingClientRect().bottom);
      const notes = document.querySelector("#notes");
      // The timer, its buttons, and the time of day, each inside the side column.
      const side = document.querySelector("aside").getBoundingClientRect().right;
      const clock = [...document.querySelector(".clock").children].map((e) => Math.round(e.getBoundingClientRect().right));
      return { height: innerHeight, back: bottom("back"), on: bottom("on"), notes: [notes.scrollHeight, notes.clientHeight], side, clock };
    });
    check(
      seen.back <= seen.height && seen.on <= seen.height && seen.notes[0] > seen.notes[1] && seen.clock.every((right) => right <= seen.side + 1),
      `at ${width} × ${height} the presenter view's notes scroll, and Back and Next and the timer stay in sight: ${JSON.stringify(seen)}`,
    );
  }
  // Its notes scrolled to their end, the next state's start at their top.
  const scrolled = await longView.evaluate(() => {
    const notes = document.querySelector("#notes");
    notes.scrollTop = notes.scrollHeight;
    return { top: notes.scrollTop, at: window.scaena.follows(), words: notes.textContent };
  });
  await longView.keyboard.press("ArrowRight");
  const topped = await longView
    .waitForFunction((was) => window.scaena.follows() !== was.at && document.querySelector("#notes").textContent !== was.words, scrolled, { timeout: 15000 })
    .then(() => longView.evaluate(() => ({ top: document.querySelector("#notes").scrollTop, at: window.scaena.follows() })), () => null);
  check(scrolled.top > 0 && topped?.top === 0, `the next state's notes start at their top: ${scrolled.top} px down on ${scrolled.at}, then ${JSON.stringify(topped)}`);
  // The player closed, the view says so and its buttons wait; a player opened again, it follows.
  await long.close();
  const closed = await longView
    .waitForFunction(() => document.querySelector("#status").textContent.startsWith("The player has closed.") && document.querySelector("#on").disabled, null, { timeout: 10000 })
    .then(() => true, () => false);
  check(closed, `once the player closes, the presenter view says so: ${await longView.textContent("#status")}`);
  const reopened = await open(url(higher));
  const followsAgain = await longView
    .waitForFunction(() => !document.querySelector("#on").disabled && document.querySelector("#status").textContent === "", null, { timeout: 30000 })
    .then(() => true, () => false);
  check(followsAgain, `and follows a player opened again: ${await longView.evaluate(() => window.scaena.follows())}`);
  // A view whose player closes as soon as the view has heard it says the player has closed, and
  // goes on saying so past the 2 s its first hello waits for an answer.
  const quick = await open(url(higher, "presenter"));
  await quick.waitForFunction(() => !document.querySelector("#on").disabled, null, { timeout: 30000 }).catch(() => {});
  await reopened.close();
  await quick.waitForTimeout(2500);
  const quickly = await quick.textContent("#status");
  check(quickly.startsWith("The player has closed."), `a player closed as its view opens is said to have closed, and stays so: ${quickly}`);
  await quick.close();
  await longView.close();

  // The notes as a speaker reads them (SPEC §2.6, lift-up USE-1): the markdown a generated deck's
  // model writes, kept as written in the deck, shows in the presenter view as a list and a table,
  // each word in its look, never a `- ` or a `|`. On a copy of the revenue example with such notes
  // on its third state, served beside the examples' fonts and themes.
  const walk = "target/notes-walk";
  await mkdir(walk, { recursive: true });
  const marked = structuredClone(deck);
  const written = "Lead with the **number**.\n- Pro is a third\n- Then the ask\n\n| Tier | Share |\n|---|---|\n| Pro | 33% |";
  marked.states[2].notes = written;
  await writeFile(join(walk, "revenue.deck.json"), JSON.stringify(marked));
  for (const dir of ["fonts", "themes", "data", "assets"]) await symlink(resolve("docs/examples", dir), join(walk, dir)).catch(() => {});
  const markedPlayer = await open(url(`/${walk}/revenue.deck.json`));
  await markedPlayer.evaluate(() => window.scaena.seek(2));
  const reader = await open(url(`/${walk}/revenue.deck.json`, "presenter"));
  await reader.waitForFunction(() => window.scaena.follows() && document.querySelector("#notes ul"), null, { timeout: 15000 }).catch(() => {});
  const read = await reader.evaluate(() => {
    const pane = document.querySelector("#notes");
    const texts = (selector) => [...pane.querySelectorAll(selector)].map((e) => e.textContent);
    return { items: texts("ul li"), bold: texts("p strong"), heads: texts("thead th"), cells: texts("tbody td"), text: pane.textContent };
  });
  check(
    read.items.join("|") === "Pro is a third|Then the ask" && read.bold.join() === "number" && read.heads.join("|") === "Tier|Share" && read.cells.join("|") === "Pro|33%" && !/^- |\|/m.test(read.text),
    `the presenter view shows notes as a speaker reads them, a list and a table, the words in their looks: ${JSON.stringify(read)}`,
  );
  await reader.screenshot({ path: join(walk, "presenter-notes.png") }).catch(() => {});
  await reader.close();
  await markedPlayer.close();

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

  // The presenter view (lift-up USE-2), in a context of its own so no other player answers it: its
  // timer waits at 0:00 while the deck stands on its first slide, and starts when the deck goes
  // on; and a presenter view the browser blocks says so in the player's status.
  const fresh = await browser.newContext({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 });
  const freshPage = async (address) => {
    const page = await fresh.newPage();
    page.on("pageerror", (e) => failures.push(`${address}: ${e.message}`));
    await page.goto(address);
    await page.waitForFunction(() => window.scaena, null, { timeout: 120000 });
    return page;
  };
  const first = await freshPage(url(revenue));
  const watching = await freshPage(url(revenue, "presenter"));
  await watching.waitForFunction(() => window.scaena.follows(), null, { timeout: 15000 }).catch(() => {});
  await watching.waitForTimeout(1500);
  const waited = await watching.evaluate(() => window.scaena.timer().shows);
  check(waited === "0:00", `the presenter view's timer waits at 0:00 while the deck stands on its first slide: ${waited}`);
  await first.keyboard.press("ArrowRight");
  const started = await watching
    .waitForFunction(() => window.scaena.timer().shows !== "0:00", null, { timeout: 5000 })
    .then(() => true, () => false);
  check(started, "and starts when the deck goes on");
  await first.evaluate(() => (window.open = () => null));
  await first.keyboard.press("p");
  const blocked = await first
    .waitForFunction(() => document.querySelector("#status").textContent.startsWith("The presenter view was blocked"), null, { timeout: 5000 })
    .then(() => true, () => false);
  check(blocked, `a presenter view the browser blocks is said: ${await first.textContent("#status")}`);
  await fresh.close();

  // Full sharpness (lift-up ATTR-2): at 2560 × 1440 and twice the density, the slide fills the
  // room the bar leaves, and in fullscreen the screen, and its frames are painted at the size it
  // is shown, in device pixels, by WebGPU; by the CPU painter at the canvas's own size, scaled up
  // to the room. The fullscreen frame is kept as target/web-smoke/sharp-2560x1440@2x.png.
  const dense = await browser.newContext({ viewport: { width: 2560, height: 1440 }, deviceScaleFactor: 2 });
  for (const painter of ["auto", "cpu"]) {
    const page = await dense.newPage();
    page.on("pageerror", (e) => failures.push(`dense ${painter}: ${e.message}`));
    await page.goto(`${server.origin}/web/dist/?painter=${painter}&bundle=${revenue}`);
    await page.waitForFunction(() => window.scaena, null, { timeout: 120000 });
    const paints = await page.evaluate(() => window.scaena.painter);
    /** The canvas's attributes, the box it is shown in, the room it has, and the density. */
    const measure = () =>
      page.evaluate(() => {
        const c = document.querySelector("#stage");
        const r = c.getBoundingClientRect();
        const bar = document.querySelector(".bar");
        const room = [innerWidth, bar.offsetParent === null ? innerHeight : innerHeight - bar.getBoundingClientRect().height];
        return { backing: [c.width, c.height], shown: [r.width, r.height], room, dpr: devicePixelRatio };
      });
    /** `m` as the painter should have it: shown as large as fits the room, filling its shorter
     * side, 16:9, and painted at the size shown in device pixels, or, by the CPU painter, at the
     * canvas's own size, 1920 × 1080, the room being larger. */
    const sharp = (m) => {
      const fills = Math.abs(m.shown[1] - m.room[1]) <= 1 || Math.abs(m.shown[0] - m.room[0]) <= 1;
      const fits = m.shown[0] <= m.room[0] + 0.5 && m.shown[1] <= m.room[1] + 0.5;
      const shape = Math.abs(m.shown[0] / m.shown[1] - 16 / 9) < 0.005 && Math.abs(m.backing[0] / m.backing[1] - 16 / 9) < 0.005;
      const device = [m.shown[0] * m.dpr, m.shown[1] * m.dpr];
      const painted =
        paints === "webgpu"
          ? Math.abs(m.backing[0] - device[0]) <= 1 && Math.abs(m.backing[1] - device[1]) <= 1
          : m.backing[0] === 1920 && m.backing[1] === 1080;
      return fills && fits && shape && painted;
    };
    // WebGPU paints at the size shown, once the page has fitted it; the CPU painter's canvas stays
    // 1920 wide, and its first frame is the deck at rest.
    if (paints === "webgpu") await page.waitForFunction(() => document.querySelector("#stage").width !== 1920, null, { timeout: 60000 }).catch(() => {});
    else await page.waitForFunction(() => window.scaena.at() && !window.scaena.at().playing, null, { timeout: 60000 }).catch(() => {});
    const windowed = await measure();
    check(sharp(windowed) && windowed.dpr === 2, `${paints}: at 2560 × 1440 at 2×, the slide fills the room the bar leaves, painted at the size shown: ${JSON.stringify(windowed)}`);
    await page.keyboard.press("f");
    const fullDense = await page.waitForFunction(() => document.fullscreenElement !== null, null, { timeout: 5000 }).then(() => true, () => false);
    if (fullDense) {
      await page.waitForFunction(() => document.querySelector(".bar").offsetParent === null, null, { timeout: 5000 }).catch(() => {});
      await page
        .waitForFunction((was) => document.querySelector("#stage").width !== was, windowed.backing[0], { timeout: 60000 })
        .catch(() => {});
      const full = await measure();
      const screen = Math.abs(full.shown[0] - 2560) <= 1 && Math.abs(full.shown[1] - 1440) <= 1;
      const whole = paints !== "webgpu" || (full.backing[0] === 5120 && full.backing[1] === 2880);
      check(sharp(full) && screen && whole, `${paints}: fullscreen, it fills the screen, painted at the size shown: ${JSON.stringify(full)}`);
      if (paints === "webgpu") {
        await mkdir("target/web-smoke", { recursive: true });
        for (let i = 0; i < 3; i++) await page.evaluate(() => new Promise(requestAnimationFrame));
        await page.screenshot({ path: "target/web-smoke/sharp-2560x1440@2x.png" });
      }
      await page.keyboard.press("f");
      await page.waitForFunction(() => document.fullscreenElement === null, null, { timeout: 5000 }).catch(() => {});
    } else {
      const why = await page.evaluate(() => ({ status: document.querySelector("#status").textContent, focus: `${document.activeElement?.tagName}#${document.activeElement?.id}` }));
      failures.push(`dense ${painter}, painted by ${paints}: F did not make the player fullscreen: ${JSON.stringify(why)}`);
    }
    await page.close();
  }
  await dense.close();
} catch (e) {
  failures.push(String(e));
} finally {
  await browser.close();
  server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "the player goes on, back, by itself, and where it is told");
process.exit(failures.length ? 1 : 0);

// A deck presented as a person presents one (PLAN 2.78): the static site's demo deck (trails),
// played and presented from the site, then exported from the editor as one HTML file and a PDF,
// each as an audience takes it. Like the first-deck and edit-deck walks, by the pages' buttons,
// menus, keys, pointer, and touch alone; `window.scaena` is only read, to wait and to check.
//
//   node web/present-deck.mjs     (after `just site`; from the repository's root)
//
// - The player: →, Space, ←, End, and Home; a click, a swipe with the mouse each way, and a tap
//   and a swipe on a touch screen; the scrubber and the state picker; a held state that goes on by
//   itself; fullscreen and back. With less motion asked for, going on cuts to the state at rest.
// - The presenter view, opened from the player: the state's notes, the next state, and its clock.
//   Its keys and buttons steer the player, and it follows the player back.
// - One HTML file, from Export…, opened from disk with the network off: it plays every state, and
//   its live region reads each as the site's player reads it.
// - A PDF, from Export…: a page for each slide, each page's text copying as the slide's texts read
//   (`web/pdf.mjs` reads it), and its outline the spine's sections.
// Each step is screenshotted into target/present-deck/. Exits 1 on any failure.
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { readPdf } from "./pdf.mjs";
import { launch, serve } from "./serve.mjs";

const out = "target/present-deck";
await rm(out, { recursive: true, force: true });
await mkdir(out, { recursive: true });
const root = join(process.cwd(), "target/site");
const deck = JSON.parse(await readFile(join(root, "decks/trails/deck.json"), "utf8"));
const media = { ".css": "text/css", ".png": "image/png", ".ttf": "font/ttf", ".csv": "text/csv", ".txt": "text/plain", ".svg": "image/svg+xml", ".webmanifest": "application/manifest+json" };
const server = await serve(root, { more: media });
const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
  return ok;
};

/** What a state's notes are, as the presenter view shows them: its own, else its beat's. */
const beats = deck.spine.sections.flatMap((s) => s.beats);
const notesOf = (state) => deck.states.find((s) => s.id === state)?.notes ?? beats.find((b) => b.states?.includes(state))?.notes ?? "No notes.";
const texts = new Set(Object.entries(deck.nodes).filter(([, n]) => n.type === "text").map(([id]) => id));

const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1, acceptDownloads: true });
  const watch = (page, name) => {
    page.on("pageerror", (e) => failures.push(`${name}: ${e.message}`));
    page.on("console", (m) => m.type() === "error" && failures.push(`${name}: console: ${m.text()}`));
    return page;
  };
  let shot = 0;
  const snap = (page, name) => page.screenshot({ path: join(out, `${String(++shot).padStart(2, "0")}-${name}.png`) }).catch(() => {});
  /** Wait for `fn(arg)` to hold in `page`. One that runs out says what it waited for, and where the
   * page's deck is then and its status line. */
  const until = async (page, what, fn, arg = null, timeout = 30000) => {
    try {
      await page.waitForFunction(fn, arg, { timeout, polling: 50 });
    } catch {
      const where = await page
        .evaluate(() => ({ at: window.scaena?.at?.() ?? window.scaena?.follows?.(), status: document.querySelector("#status")?.textContent }))
        .catch(() => ({}));
      throw new Error(`${what}: not within ${timeout / 1000} s · at ${JSON.stringify(where.at)} · ${where.status}`);
    }
  };
  const at = (page) => page.evaluate(() => window.scaena.at());
  /** Once `page`'s deck shows state `index` at rest: its cue played. The deck's clock runs on
   * through the state's hold (`playing`), so at rest is past the cue's span. */
  const rests = (page, index, what = `state ${index} at rest`) =>
    until(page, what, ([i, span]) => window.scaena.at().index === i && window.scaena.at().t >= span, [index, slots[index]?.span ?? 0]);
  /** Once `page`'s deck shows state `index`, playing or at rest. A page behind another window
   * may get no frames, so its cue may not play out while the presenter view is in front: the
   * state it went to is what steering it says. */
  const goes = (page, index, what = `state ${index}`) => until(page, what, (i) => window.scaena.at().index === i, index);
  const step = async (name, page, act) => {
    const started = Date.now();
    try {
      await act();
    } catch (e) {
      check(false, `${name}: ${String(e).split("\n")[0]}`);
    }
    console.log(`     ${name}: ${((Date.now() - started) / 1000).toFixed(1)} s`);
    await snap(page(), name);
  };

  const player = watch(await context.newPage(), "player");
  let slots = [];
  let states = [];
  await step("open", () => player, async () => {
    await player.goto(`${server.origin}/index.html?painter=cpu`);
    await until(player, "the player opens", () => window.scaena?.at, null, 120000);
    slots = await player.evaluate(() => window.scaena.timeline());
    states = slots.map((s) => s.state);
    check(states.length === deck.states.length && states[0] === "cover", `the site's player opens on its demo deck: ${states.length} states, ${states[0]} first`);
    check((await at(player)).index === 0, "at the first state");
  });

  // 1. The keys. A state at rest holds a few seconds, then goes on by itself: each key comes
  // once the one before has come to rest, well within the hold.
  await step("keys", () => player, async () => {
    await player.keyboard.press("ArrowRight");
    await rests(player, 1, "→ goes on");
    await player.keyboard.press(" ");
    await rests(player, 2, "Space goes on");
    await player.keyboard.press("ArrowLeft");
    await rests(player, 1, "← goes back");
    await player.keyboard.press("End");
    await rests(player, states.length - 1, "End goes to the last");
    await player.keyboard.press("Home");
    await rests(player, 0, "Home goes to the first");
    check(true, "→, Space, ←, End, and Home each go where they say");
  });

  // 2. The pointer: a click goes on, a swipe left goes on, a swipe right goes back.
  const middle = async (page) => {
    const r = await page.locator("#stage").boundingBox();
    return [r.x + r.width / 2, r.y + r.height / 2];
  };
  await step("pointer", () => player, async () => {
    const [x, y] = await middle(player);
    await player.mouse.click(x, y);
    await rests(player, 1, "a click goes on");
    const swipe = async (dx) => {
      await player.mouse.move(x, y);
      await player.mouse.down();
      await player.mouse.move(x + dx, y + 10, { steps: 6 });
      await player.mouse.up();
    };
    await swipe(-400);
    await rests(player, 2, "a swipe left goes on");
    await swipe(400);
    await rests(player, 1, "a swipe right goes back");
    check(true, "a click and a swipe left go on, a swipe right goes back");
  });

  // 3. The scrubber, by a click on it and by its keys, and the state picker.
  await step("scrubber", () => player, async () => {
    // The scrubber's steps are the states: a click in the fifth step shows the fifth state.
    const bar = await player.locator("#scrub").boundingBox();
    await player.mouse.click(bar.x + (bar.width * 4.5) / states.length, bar.y + bar.height / 2);
    await until(player, "a click on the scrubber", () => window.scaena.at().index === 4);
    check(true, `a click in the scrubber's fifth step shows ${states[4]}`);
    // End on the scrubber: the last state at rest.
    await player.locator("#scrub").press("End");
    await rests(player, states.length - 1, "End on the scrubber");
    check(true, `End on the scrubber shows ${states.at(-1)} at rest`);
    const options = await player.$$eval("#state option", (o) => o.map((x) => x.textContent));
    check(options.length === states.length && options.some((o) => / · /.test(o)), `the state picker lists each state, with its beat's claim: ${options[4]}`);
    await player.selectOption("#state", "5");
    await rests(player, 5, "the state picker");
    check(true, `the state picker plays ${states[5]}`);
  });

  // 4. A held state goes on by itself.
  await step("hold", () => player, async () => {
    // The deck's clock says how long into the state it is: its cue, then its hold so far.
    const { index: from, t } = await at(player);
    const held = Math.max(0, t - slots[from].span);
    const started = Date.now();
    await until(player, `${states[from]}'s hold ends`, (i) => window.scaena.at().index === i + 1, from, slots[from].hold + 5000);
    const waited = held + Date.now() - started;
    check(waited >= slots[from].hold - 300, `${states[from]} holds ${slots[from].hold} ms, then goes on to ${states[from + 1]} by itself: ${Math.round(waited)} ms`);
  });

  // 5. The presenter view, before fullscreen.
  let presenter;
  await step("presenter", () => presenter ?? player, async () => {
    await player.keyboard.press("Home");
    // Home shows the first state and stays there: a state sought does not play on.
    await goes(player, 0, "Home");
    // The presenter view opens as a window of its own, on a click on Presenter once the player
    // has it ready; a slow machine takes a while to show it.
    const present = player.locator("#present");
    await present.waitFor({ state: "visible", timeout: 30000 });
    const opening = context.waitForEvent("page", { timeout: 120000 });
    await present.click();
    presenter = await opening;
    watch(presenter, "presenter");
    const follows = (i) => `${i + 1} / ${states.length} · ${states[i]}`;
    const followed = (i) => until(presenter, `the presenter view follows to ${states[i]}`, (w) => window.scaena?.follows?.() === w, follows(i), 120000);
    await followed(0);
    check((await presenter.textContent("#notes")) === notesOf(states[0]), `it shows ${states[0]}'s notes: ${await presenter.textContent("#notes")}`);
    check((await presenter.textContent("#nextName")) === `Next: ${states[1]}`, `and ${states[1]} next`);
    const clock = await presenter.textContent("#clock");
    await until(presenter, "its clock runs", (c) => document.querySelector("#clock").textContent !== c, clock, 5000);
    check(true, `its clock runs: ${clock} → ${await presenter.textContent("#clock")}`);
    // Its keys and buttons steer the player. Going on plays the next state's cue and then its
    // hold, after which the deck goes on by itself; going back, End, and Home show a state at
    // rest, where it stays. So each going on starts at rest and goes back at once: the walk
    // never races the deck's own holds, nor needs the cue to play out, which a player behind the
    // presenter view's window may get no frames for.
    await presenter.bringToFront();
    const steer = async (act, to, what) => {
      await act();
      await goes(player, to, what);
    };
    await steer(() => presenter.keyboard.press("ArrowRight"), 1, "the presenter view's → steers the player");
    await steer(() => presenter.keyboard.press("ArrowLeft"), 0, "the presenter view's ← steers the player");
    await steer(() => presenter.keyboard.press(" "), 1, "the presenter view's Space steers the player");
    await steer(() => presenter.keyboard.press("ArrowLeft"), 0, "and back");
    check(true, "its →, ←, and Space steer the player");
    await steer(() => presenter.click("#on"), 1, "its ▶");
    await steer(() => presenter.click("#back"), 0, "its ◀");
    check(true, "its ▶ and ◀ steer the player");
    await steer(() => presenter.keyboard.press("End"), states.length - 1, "the presenter view's End");
    await steer(() => presenter.keyboard.press("Home"), 0, "the presenter view's Home");
    check(true, "its End and Home go to the last and the first, as the player's do");
    // The presenter view follows the player where the player is taken: a click in the
    // scrubber's fifth step, which shows that state and stays there. A click lands where it is
    // pressed, while a key goes where the focus is, which the presenter view's window may hold.
    await player.bringToFront();
    const bar = await player.locator("#scrub").boundingBox();
    await player.mouse.click(bar.x + (bar.width * 4.5) / states.length, bar.y + bar.height / 2);
    await goes(player, 4, "a click on the player's scrubber");
    await followed(4);
    check((await presenter.textContent("#notes")) === notesOf(states[4]) && (await presenter.textContent("#nextName")) === `Next: ${states[5]}`, `it follows the player to ${states[4]}: its notes, and ${states[5]} next`);
    await snap(presenter, "presenter-view");
  });
  await presenter?.close();
  // 6. Fullscreen, by the button, and back by F: after the presenter view, whose window a page
  // leaving fullscreen may not get to open.
  await step("fullscreen", () => player, async () => {
    await player.click("#full");
    await until(player, "the button makes it fullscreen", () => document.fullscreenElement !== null, null, 5000);
    await player.locator("#stage").focus().catch(() => {});
    await player.keyboard.press("f");
    await until(player, "F leaves fullscreen", () => document.fullscreenElement === null, null, 5000);
    check(true, "⛶ goes fullscreen, and F comes back");
  });

  await player.close();

  // 7. Less motion: going on cuts to the state at rest, with no frame inside a cue.
  const still = await browser.newContext({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1, reducedMotion: "reduce" });
  const calm = watch(await still.newPage(), "less motion");
  await step("less-motion", () => calm, async () => {
    await calm.goto(`${server.origin}/index.html?painter=cpu`);
    await until(calm, "the player opens", () => window.scaena?.at, null, 120000);
    // Where the deck is at each frame the page draws.
    await calm.evaluate(() => {
      window.seen = [];
      const tick = () => {
        window.seen.push(window.scaena.at());
        requestAnimationFrame(tick);
      };
      requestAnimationFrame(tick);
    });
    await calm.keyboard.press("ArrowRight");
    await until(calm, "→ with less motion", () => window.scaena.at().index === 1);
    await calm.waitForTimeout(slots[1].span + 500);
    const seen = await calm.evaluate(() => window.seen.filter((s) => s.index === 1));
    const inCue = seen.filter((s) => s.t < slots[1].span);
    check(seen.length > 0 && !inCue.length, `with less motion asked for, → cuts to ${states[1]} at rest: ${inCue.length} of ${seen.length} frames inside its cue`);
  });
  await still.close();

  // 8. Touch: a tap goes on, a swipe goes back.
  const touch = await browser.newContext({ viewport: { width: 900, height: 600 }, deviceScaleFactor: 1, hasTouch: true });
  const phone = watch(await touch.newPage(), "touch");
  await step("touch", () => phone, async () => {
    await phone.goto(`${server.origin}/index.html?painter=cpu`);
    await until(phone, "the player opens", () => window.scaena?.at, null, 120000);
    const [x, y] = await middle(phone);
    await phone.touchscreen.tap(x, y);
    await rests(phone, 1, "a tap goes on");
    const cdp = await touch.newCDPSession(phone);
    const finger = (type, px) => cdp.send("Input.dispatchTouchEvent", { type, touchPoints: type === "touchEnd" ? [] : [{ x: px, y }] });
    await finger("touchStart", x - 150);
    for (let k = 1; k <= 6; k++) await finger("touchMove", x - 150 + 50 * k);
    await finger("touchEnd");
    await rests(phone, 0, "a swipe right goes back");
    check(true, "on a touch screen, a tap goes on and a swipe right goes back");
  });
  await touch.close();

  // 9. The exports, from the editor's Export…, as the audience gets them.
  const editor = watch(await context.newPage(), "editor");
  const downloaded = async (as) => {
    await editor.click("#export");
    await editor.check(`#exporting input[name=as][value=${as}]`);
    const [download] = await Promise.all([editor.waitForEvent("download", { timeout: 180000 }), editor.click("#exporting-go")]);
    const file = join(out, download.suggestedFilename());
    await writeFile(file, await readFile(await download.path()));
    return file;
  };
  let html;
  let pdf;
  await step("export", () => editor, async () => {
    await editor.goto(`${server.origin}/editor.html?painter=cpu`);
    await until(editor, "the editor opens", () => window.scaena?.last()?.valid, null, 120000);
    html = await downloaded("html");
    pdf = await downloaded("pdf");
    check(html.endsWith(".html") && pdf.endsWith(".pdf"), `Export… gives ${html} and ${pdf}`);
  });
  await editor.close();

  // 10. The one file, from disk, with the network off: every state plays, and reads.
  const readings = [];
  const offline = await browser.newContext({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1 });
  await offline.setOffline(true);
  const file = watch(await offline.newPage(), "the file");
  await step("single-file", () => file, async () => {
    await file.goto(`${pathToFileURL(resolve(html)).href}?painter=cpu`);
    await until(file, "the file opens", () => window.scaena?.at, null, 120000);
    check((await file.getAttribute("#reading", "aria-live")) === "polite", "its reading is a live region");
    const reading = () => file.evaluate(() => [...document.querySelectorAll("#reading > *")].map((el) => ({ node: el.dataset.node, text: el.textContent })));
    for (let i = 0; i < states.length; i++) {
      if (i > 0) await file.keyboard.press("ArrowRight");
      await until(file, `the file goes on to ${states[i]}`, (k) => window.scaena.at().index === k, i);
      // Its reading comes once the state is shown: wait for it to be another than the last.
      const last = JSON.stringify(readings.at(-1) ?? []);
      await until(file, `${states[i]} reads`, (l) => {
        const now = [...document.querySelectorAll("#reading > *")].map((el) => ({ node: el.dataset.node, text: el.textContent }));
        return now.length > 0 && JSON.stringify(now) !== l;
      }, last);
      readings.push(await reading());
      await rests(file, i, `${states[i]} at rest in the file`);
    }
    check(readings.length === states.length && readings.every((r) => r.length > 0), `the file plays all ${states.length} states, and its live region reads each`);
  });
  await offline.close();

  // 11. The PDF: a page for each slide, its text as the slide reads, its outline the spine's.
  await step("pdf", () => file, async () => {
    const { pages, outline } = readPdf(await readFile(pdf));
    check(pages.length === states.length, `a page for each slide: ${pages.length}`);
    const sections = deck.spine.sections.map((s) => s.title);
    check(JSON.stringify(outline.map((o) => o.title)) === JSON.stringify(sections), `its outline is the spine's sections: ${outline.map((o) => o.title).join(" · ")}`);
    // Each text the slide reads is on its page, as it copies: whitespace and the hyphens a line
    // break adds aside. A role in capitals copies as written (PLAN 2.88).
    const bare = (s) => s.replace(/[\s\u00ad-]+/g, "");
    const missing = [];
    pages.forEach((page, i) => {
      for (const { node, text } of readings[i] ?? []) if (texts.has(node) && !bare(page.text).includes(bare(text))) missing.push(`${states[i]}: ${node} "${text}"`);
    });
    check(!missing.length, `each page's text copies as its slide's texts read${missing.length ? `; not found: ${missing.join("; ")}` : ""}`);
  });
} catch (e) {
  failures.push(String(e));
} finally {
  await browser.close();
  server.close();
}

if (failures.length) {
  console.log(`\n${failures.length} failed:\n  ${failures.join("\n  ")}`);
  process.exit(1);
}
console.log("\na deck presented: played by keys, pointer, and touch, steered from the presenter view, read from one file offline, and printed");

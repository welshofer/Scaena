// PLAN 2.112 check: a presentation app's builds, offered by the cue's Add an animation… menu, in
// headless Chromium (serve.mjs), the CPU painting, on the revenue example (Dusk).
//
//   node web/builds.mjs     (after `just web`; from the repository's root)
//
// - In `close`, the backdrop selected: the menu offers the shipped builds as a presentation app
//   names them (Appear, Pop, Drop, Move In from Left, Spin, Flip, Blink, Jiggle).
// - Each build splits a node only into what it has (E106): the backdrop does not type out, nor
//   grow by its marks, while the note that leaves types out and the chart that leaves takes Grow.
// - Each kind offers its own (PLAN 2.116): an entrance and an exit the builds that start from a
//   look, emphasis those that go to one and back. Fade enters and Pulse emphasizes, never the other
//   way round, and Pop does both. Build Out names each as an exit: Move Out to Left, Disappear.
// - Its bar moved later by a key makes it a call to Pop that waits, and Build In still says Pop.
// - Pop chosen is one `apply_preset` patch: the backdrop enters on Pop, its bar in the cue, on the
//   bouncy spring's time.
// - Jiggle for emphasis is another, its own bar.
// - Each one step to undo; undone, the deck is as it was, and it validates.
// - By Bullet (PLAN 2.129): the note typed as three paragraphs, Build In and Build Out offer By
//   Bullet. Chosen as its entrance, one patch adds a step for each paragraph after the first: the
//   slide draws the first, the next click the second, the last all three, the status counting the
//   steps on the slide, and the second click's cue rises its new paragraph in. Built in so, Build
//   Out's By Bullet adds a step after `mix` for each paragraph it lets go, and one undo takes them
//   back. Rise chosen as its entrance takes By Bullet back, as All at Once does: its steps go and
//   the note enters whole, one undo building it by bullet again. One undo takes the build and its
//   steps back. In `close`, which the note leaves, the cue's menu offers it By Bullet as it leaves,
//   and nothing else: one patch, a step after `mix` for each paragraph it lets go, one undo.
// Exits 1 on any failure.
import { addAnimation, launch, serve } from "./serve.mjs";

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
  const says = (what) =>
    page.waitForFunction((w) => document.querySelector("#status").textContent.includes(w), what, { timeout: 30000 }).then(
      () => true,
      () => false,
    );
  const redrawn = (before) =>
    page.waitForFunction((n) => window.scaena.trips().length > n, before, { timeout: 60000, polling: 50 }).then(() => page.waitForTimeout(150));
  const undo = async () => {
    const n = await trips();
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+z");
    await redrawn(n);
  };
  const original = await source();

  // In `close`, the backdrop selected.
  const index = await page.evaluate(() => window.scaena.opened.states.indexOf("close"));
  await page.evaluate(() => window.scaena.cursor(window.scaena.source().indexOf("state close") + "state ".length));
  await page.waitForFunction((i) => window.scaena.shown() === i && window.scaena.at().index === i, index, { timeout: 30000 });
  await page.evaluate(() => window.scaena.canvas.select("bg"));
  await page.waitForFunction(() => window.scaena.cue.offers().some((o) => JSON.stringify(o.value).includes('"enter","bg","pop"')), null, {
    timeout: 30000,
  });
  // The backdrop's own builds: the menu also offers each object that leaves its own exits, a
  // text's among them.
  const offered = await page.evaluate(() =>
    window.scaena.cue.offers().filter((o) => o.value[1] === "bg").map((o) => o.label),
  );
  const wanted = ["Appear", "Pop", "Fade and Scale", "Drop", "Move In from Left", "Move In from Bottom", "Spin", "Swing", "Flip", "Blink", "Jiggle"];
  check(
    wanted.every((w) => offered.includes(w)) && !offered.includes("Typewriter") && !offered.includes("Word by Word"),
    `the menu offers a presentation app's builds, in its words, and none that plays a text's letters or words: ${wanted.filter((w) => !offered.includes(w)).join(", ") || "all of them"}`,
  );
  const values = await page.evaluate(() => window.scaena.cue.offers().map((o) => JSON.stringify(o.value)));
  const offers = (kind, node, preset) => values.includes(JSON.stringify([kind, node, preset]));
  check(
    !["typewriter", "words", "grow"].some((p) => offers("enter", "bg", p)),
    "the backdrop neither types out, nor comes in word by word, nor grows by its marks",
  );
  check(offers("exit", "note", "typewriter") && offers("exit", "note", "words"), "the note that leaves types out, as text does");
  check(offers("exit", "rev", "grow"), "and the chart that leaves takes Grow on its marks");
  const kinds = await page.evaluate(() => {
    const by = { enter: [], emphasis: [], exit: [] };
    for (const o of window.scaena.cue.offers()) {
      const [kind, , preset] = o.value;
      by[kind]?.push(preset);
    }
    return by;
  });
  const has = (kind, preset) => kinds[kind].includes(preset);
  check(
    has("enter", "fade") && !has("emphasis", "fade"),
    `Fade enters but does not emphasize: ${kinds.emphasis.join(", ")}`,
  );
  check(
    ["pulse", "blink", "jiggle"].every((p) => has("emphasis", p) && !has("enter", p) && !has("exit", p)),
    `Pulse, Blink, and Jiggle emphasize, and neither enter nor leave: ${kinds.enter.join(", ")}`,
  );
  check(has("enter", "pop") && has("emphasis", "pop"), "and Pop does both");
  // An exit is named as a Build Out effect is, in the inspector and the cue alike: the entrance
  // played back.
  await page.waitForSelector("#look-exit option", { state: "attached", timeout: 30000 }).catch(() => {});
  const exits = await page.evaluate(() => [...document.querySelectorAll("#look-exit option")].map((o) => o.textContent));
  check(
    exits.includes("Move Out to Left") && exits.includes("Disappear") && !exits.includes("Move In from Left"),
    `Build Out names exits: ${exits.join(", ")}`,
  );

  // Pop, as it enters.
  let n = await trips();
  await addAnimation(page, '["enter","bg","pop"]');
  check((await says("Pop added to")) && (await status()).includes("as it enters"), `Pop is added as the backdrop enters: ${await status()}`);
  await redrawn(n);
  const popped = await bar("bg enter");
  check(popped !== undefined, "the cue has its bar");
  const span = popped ? popped.to - popped.from : 0;
  check(popped?.sprung && span > 300 && span < 1500, `on the bouncy spring's time: ${span} ms`);
  check(/bg[^\n]*enter:pop/.test(await source()), "written as the backdrop's entrance");
  const added = await source();

  // Its bar moved later by a key: the entrance is a call to Pop with a wait, and the inspector's
  // Build In still says Pop, not Custom.
  const at = (await bars()).findIndex((b) => b.key === "bg enter");
  n = await trips();
  await page.locator(`#cue .bar[data-bar="${at}"]`).focus();
  await page.keyboard.press("Shift+ArrowRight");
  await redrawn(n);
  check(/bg[^\n]*enter:\{preset: ?pop, ?delay: ?100\}/.test(await source()), `written as a call to Pop that waits: ${(await source()).match(/bg [^\n]*/)?.[0]}`);
  await page.evaluate(() => window.scaena.canvas.select("bg"));
  const build = await page
    .waitForFunction(() => document.querySelector("#look-enter")?.selectedOptions[0]?.textContent, null, { timeout: 30000 })
    .then((h) => h.jsonValue())
    .catch(() => undefined);
  check(build === "Pop", `Build In still says Pop: ${build}`);
  await undo();
  check((await source()) === added, "one undo takes the wait back");

  // Jiggle, for emphasis.
  await page.waitForFunction(() => window.scaena.cue.offers().some((o) => JSON.stringify(o.value).includes('"emphasis","bg","jiggle"')), null, {
    timeout: 30000,
  });
  n = await trips();
  await addAnimation(page, '["emphasis","bg","jiggle"]');
  check((await says("Jiggle added to")) && (await status()).includes("for emphasis"), `Jiggle is added for emphasis: ${await status()}`);
  await redrawn(n);
  check((await bar("bg emphasis")) !== undefined, "with its own bar");

  // Each one step to undo.
  await undo();
  check((await source()) === added, "one undo takes Jiggle away");
  await undo();
  check((await source()) === original, "and another Pop: the deck is as it was");
  check(await page.evaluate(() => window.scaena.last().valid), "and it validates");

  // By Bullet (PLAN 2.129): the note typed as three paragraphs, then built in a paragraph a click.
  const note = '"Revenue in $M. Enterprise recognized on delivery."';
  check(original.includes(note), "the note's words are in the source");
  const listed = original.replace(note, '"Plants\\nChlorophyll\\nWater"');
  await page.evaluate((s) => window.scaena.type(s), listed);
  await page.waitForFunction((s) => window.scaena.source() === s && window.scaena.last()?.valid, listed, { timeout: 60000, polling: 100 });
  const revenue = await page.evaluate(() => window.scaena.last().states.findIndex((s) => s[0] === "revenue"));
  await page.evaluate(() => window.scaena.cursor(window.scaena.source().indexOf("state revenue") + "state ".length));
  await page.waitForFunction((i) => window.scaena.shown() === i && window.scaena.at().index === i, revenue, { timeout: 30000 });
  await page.evaluate(() => window.scaena.canvas.select("note"));
  await page.waitForFunction(() => window.scaena.look.offered()?.node === "note", null, { timeout: 30000 });
  await page.waitForSelector('#look-enter option[value="bullets"]', { state: "attached", timeout: 30000 }).catch(() => {});
  const said = (id) => page.evaluate((i) => [...document.querySelectorAll(`#${i} option`)].find((o) => o.value === "bullets")?.textContent, id);
  check((await said("look-enter")) === "By Bullet", `Build In offers By Bullet for a text of three paragraphs: ${await said("look-enter")}`);
  check((await said("look-exit")) === "By Bullet", `and so does Build Out: ${await said("look-exit")}`);
  n = await trips();
  await page.selectOption("#look-enter", "bullets");
  await redrawn(n);
  await page
    .waitForFunction((b) => window.scaena.last().states.map((s) => s[0]).join(" ") === b, "intro revenue revenue-2 revenue-3 mix close", { timeout: 30000 })
    .catch(() => {});
  const steps = await page.evaluate(() => window.scaena.last().states.map((s) => s[0]).join(" "));
  check(steps === "intro revenue revenue-2 revenue-3 mix close", `By Bullet adds a step for each paragraph after the first: ${steps}`);
  check(/^Build In: By Bullet · on \d steps of Slide 2\b/.test(await status()), `the status counts the steps it adds on the slide's: ${await status()}`);
  const reads = (state) => page.evaluate((s) => window.scaena.reading(s), state);
  const [first, second, third] = [await reads("revenue"), await reads("revenue-2"), await reads("revenue-3")];
  check(first.includes("Plants") && !first.includes("Chlorophyll"), `the slide draws the first paragraph: ${first.replace(/<[^>]+>/g, " ")}`);
  check(second.includes("Chlorophyll") && !second.includes("Water"), "the next click the second");
  check(third.includes("Plants") && third.includes("Water"), "and the last all three");
  check(/note[^\n]*enter:bullets/.test(await source()) && (await source()).includes("paragraphs:[1, 1]"), "written as the note's entrance and the paragraphs each step draws");
  // On the second click the paragraph it adds rises in by the build, at once, while the first
  // stays (the slide's own entrance for the note is the choreographed fade, 600 ms in).
  const secondClick = await page.evaluate(() => window.scaena.last().states.findIndex((s) => s[0] === "revenue-2"));
  await page.evaluate(() => window.scaena.cursor(window.scaena.source().indexOf("state revenue-2") + "state ".length));
  await page.waitForFunction((i) => window.scaena.shown() === i && window.scaena.at().index === i, secondClick, { timeout: 30000 }).catch(() => {});
  await page
    .waitForFunction(() => window.scaena.cue.bars().some((b) => b.key === "note enter" && b.delay === 0), null, { timeout: 30000 })
    .catch(() => {});
  const rise = await bar("note enter");
  check(rise?.delay === 0 && rise.to > rise.from, `the second click's cue has the note's paragraph rising in: ${JSON.stringify(rise)}`);
  // Built in so, it builds out by bullet too, from Build Out: a step after `mix` for each paragraph
  // it lets go.
  const built = await source();
  const lookAt = async (node) => {
    await page.evaluate((id) => window.scaena.canvas.select(id), node);
    await page.waitForFunction((id) => window.scaena.look.offered()?.node === id, node, { timeout: 30000 });
  };
  const states = () => page.evaluate(() => window.scaena.last().states.map((s) => s[0]).join(" "));
  await lookAt("note");
  n = await trips();
  await page.selectOption("#look-exit", "bullets");
  await redrawn(n);
  const bothWanted = "intro revenue revenue-2 revenue-3 mix revenue-4 revenue-5 revenue-6 close";
  await page.waitForFunction((w) => window.scaena.last().states.map((s) => s[0]).join(" ") === w, bothWanted, { timeout: 30000 }).catch(() => {});
  const both = await states();
  check(both === bothWanted, `built in by bullet, it builds out by bullet too: ${both}`);
  await undo();
  check((await source()) === built, "one undo takes the build out back");
  // Another entrance chosen takes By Bullet back, as All at Once does: its steps go, and the note
  // enters whole, by Rise.
  await lookAt("note");
  n = await trips();
  await page.selectOption("#look-enter", "rise");
  await redrawn(n);
  await page.waitForFunction(() => window.scaena.last().states.map((s) => s[0]).join(" ") === "intro revenue mix close", null, { timeout: 30000 }).catch(() => {});
  const whole = await states();
  check(whole === "intro revenue mix close", `Rise chosen takes By Bullet's steps back: ${whole}`);
  check(!(await source()).includes("paragraphs:") && /note[^\n]*enter:rise/.test(await source()), "and the note enters whole, by Rise");
  check((await reads("revenue")).includes("Water"), "the slide draws all of it");
  check(await page.evaluate(() => window.scaena.last().valid), "and it validates");
  await undo();
  check((await source()) === built, "one undo builds it by bullet again");

  // One step to undo: the build and its steps.
  await undo();
  check((await source()) === listed, "one undo takes By Bullet and its steps back");
  check(await page.evaluate(() => window.scaena.last().valid), "and it validates");

  // Out by bullet from the cue: in `close`, which the note leaves, Add an animation… offers it By
  // Bullet as it leaves, one patch that adds a step after `mix` for each paragraph it lets go.
  const close = await page.evaluate(() => window.scaena.last().states.findIndex((s) => s[0] === "close"));
  await page.evaluate(() => window.scaena.cursor(window.scaena.source().indexOf("state close") + "state ".length));
  await page.waitForFunction((i) => window.scaena.shown() === i && window.scaena.at().index === i, close, { timeout: 30000 });
  const leaving = '["exit","note","bullets"]';
  await page
    .waitForFunction((v) => window.scaena.cue.offers().some((o) => JSON.stringify(o.value) === v), leaving, { timeout: 30000 })
    .catch(() => {});
  const cued = await page.evaluate((v) => window.scaena.cue.offers().find((o) => JSON.stringify(o.value) === v)?.label, leaving);
  check(cued === "By Bullet", `Add an animation… offers the note By Bullet as it leaves: ${cued}`);
  const bulletless = await page.evaluate(() =>
    window.scaena.cue.offers().some((o) => JSON.stringify(o.value).startsWith('["exit",') && JSON.stringify(o.value).endsWith(',"bullets"]') && !JSON.stringify(o.value).includes('"note"')),
  );
  check(!bulletless, "and nothing else that leaves, none a text of paragraphs");
  n = await trips();
  await addAnimation(page, leaving);
  await redrawn(n);
  await page.waitForFunction((l) => window.scaena.source() !== l, listed, { timeout: 30000 }).catch(() => {});
  const out = await page.evaluate(() => window.scaena.last().states.map((s) => s[0]).join(" "));
  check(out === "intro revenue mix revenue-2 revenue-3 revenue-4 close", `a step after mix for each paragraph it lets go: ${out}`);
  const [less, last] = [await reads("revenue-3"), await reads("revenue-4")];
  check(!less.includes("Plants") && !less.includes("Chlorophyll") && less.includes("Water"), "one fewer each click, from the first");
  check(!last.includes("Water"), "and then it is gone");
  await undo();
  check((await source()) === listed, "one undo takes it back");
} catch (e) {
  failures.push(`error: ${e.message}`);
} finally {
  await browser.close();
  server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "a presentation app's builds are offered, added, and undone");
process.exit(failures.length ? 1 : 0);

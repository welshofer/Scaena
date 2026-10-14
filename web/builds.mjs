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
//   way round, and Pop does both.
// - Its bar moved later by a key makes it a call to Pop that waits, and Build In still says Pop.
// - Pop chosen is one `apply_preset` patch: the backdrop enters on Pop, its bar in the cue, on the
//   bouncy spring's time.
// - Jiggle for emphasis is another, its own bar.
// - Each one step to undo; undone, the deck is as it was, and it validates.
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
  await page.waitForFunction(() => [...document.querySelectorAll("#cue [data-add] option")].some((o) => o.value.includes('"enter","bg","pop"')), null, {
    timeout: 30000,
  });
  const offered = await page.evaluate(() => [...document.querySelectorAll("#cue [data-add] option")].map((o) => o.textContent));
  const wanted = ["Appear", "Pop", "Fade and Scale", "Drop", "Move In from Left", "Move In from Bottom", "Spin", "Swing", "Flip", "Blink", "Jiggle"];
  check(
    wanted.every((w) => offered.includes(w)) && !offered.includes("Typewriter") && !offered.includes("Word by Word"),
    `the menu offers a presentation app's builds, in its words, and none that plays a text's letters or words: ${wanted.filter((w) => !offered.includes(w)).join(", ") || "all of them"}`,
  );
  const values = await page.evaluate(() => [...document.querySelectorAll("#cue [data-add] option")].map((o) => o.value));
  const offers = (kind, node, preset) => values.includes(JSON.stringify([kind, node, preset]));
  check(
    !["typewriter", "words", "grow"].some((p) => offers("enter", "bg", p)),
    "the backdrop neither types out, nor comes in word by word, nor grows by its marks",
  );
  check(offers("exit", "note", "typewriter") && offers("exit", "note", "words"), "the note that leaves types out, as text does");
  check(offers("exit", "rev", "grow"), "and the chart that leaves takes Grow on its marks");
  const kinds = await page.evaluate(() => {
    const by = { enter: [], emphasis: [], exit: [] };
    for (const o of document.querySelectorAll("#cue [data-add] option")) {
      if (!o.value.startsWith("[")) continue;
      const [kind, , preset] = JSON.parse(o.value);
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

  // Pop, as it enters.
  let n = await trips();
  await page.locator("#cue [data-add]").selectOption('["enter","bg","pop"]');
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
  await page.waitForFunction(() => [...document.querySelectorAll("#cue [data-add] option")].some((o) => o.value.includes('"emphasis","bg","jiggle"')), null, {
    timeout: 30000,
  });
  n = await trips();
  await page.locator("#cue [data-add]").selectOption('["emphasis","bg","jiggle"]');
  check((await says("Jiggle added to")) && (await status()).includes("for emphasis"), `Jiggle is added for emphasis: ${await status()}`);
  await redrawn(n);
  check((await bar("bg emphasis")) !== undefined, "with its own bar");

  // Each one step to undo.
  await undo();
  check((await source()) === added, "one undo takes Jiggle away");
  await undo();
  check((await source()) === original, "and another Pop: the deck is as it was");
  check(await page.evaluate(() => window.scaena.last().valid), "and it validates");
} catch (e) {
  failures.push(`error: ${e.message}`);
} finally {
  await browser.close();
  server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "a presentation app's builds are offered, added, and undone");
process.exit(failures.length ? 1 : 0);

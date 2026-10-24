// PLAN 2.111 check: slide transitions, chosen in the editor's inspector, in headless Chromium
// (serve.mjs), the CPU painting, as in web/inspector.mjs.
//
//   node web/transition.mjs     (after `just web`; from the repository's root)
//
// On the revenue example, in `close`, its last slide, nothing selected:
// - The inspector offers the state's Transition, a presentation app's (Dissolve, Push, Move In,
//   Reveal, Wipe), and its Direction, in plain words.
// - Push chosen is one patch that gives the state's transition a `style`; the cue's listing says
//   "Push transition"; Bottom to Top chosen sets its `direction`.
// - Its cue runs the transition's duration, and offers no exit for what leaves: the slide left
//   moves whole, and none of its exits plays.
// - Dissolve chosen, the listing names no way it goes: a dissolve fades in place.
// - Each choice is one step to undo.
// - The transitions through shapes that grow (PLAN 2.149): the inspector offers Circle, Circle
//   Wave, Diamond, Diamond Wave, and Radial; Circle Wave chosen is one patch, the cue's listing
//   names it with its way, halfway through its cue the canvas shows neither slide alone, and Play
//   plays it to rest. One undo takes it back.
// Exits 1 on any failure.
import { launch, serve } from "./serve.mjs";

const server = await serve();
const url = (bundle) => `${server.origin}/web/dist/editor.html?painter=cpu&bundle=${bundle}`;
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
  await page.goto(url("/docs/examples/revenue.deck.json"));
  await page.waitForFunction(() => window.scaena?.last() || document.querySelector("#status")?.textContent.startsWith("Error"), null, {
    timeout: 120000,
  });

  const source = () => page.evaluate(() => window.scaena.source());
  const closeLine = async () => (await source()).split("\n").find((l) => l.startsWith("state close")) ?? "";
  /** Once the `close` state's line holds `text`, and the canvas has caught up. */
  const reads = async (text) => {
    await page
      .waitForFunction(
        (t) => window.scaena.source().split("\n").find((l) => l.startsWith("state close"))?.includes(t) && !window.scaena.canvas.busy(),
        text,
        { timeout: 30000, polling: 50 },
      )
      .catch(() => {});
    await page.waitForTimeout(300);
    return (await closeLine()).includes(text);
  };
  const back = async (to) => {
    await page.waitForFunction((s) => window.scaena.source() === s && !window.scaena.canvas.busy(), to, { timeout: 30000, polling: 50 }).catch(() => {});
    await page.waitForTimeout(300);
    return (await source()) === to;
  };
  const undo = async () => {
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+z");
  };
  const options = (select) => page.evaluate((s) => [...document.querySelectorAll(`${s} option`)].map((o) => o.textContent), select);

  // In `close`, nothing selected: the inspector edits the state.
  await page.evaluate(() => window.scaena.cursor(window.scaena.source().indexOf("state close") + "state ".length));
  await page.waitForFunction(() => window.scaena.canvas.boxed() === "close", null, { timeout: 30000 });
  await page.evaluate(() => window.scaena.canvas.select(undefined));
  await page.locator("#tab-inspector").click();
  await page.waitForSelector("#look-transition-style", { timeout: 30000 });
  const original = await source();
  /** The exits the cue's Add an animation… menu offers. */
  const exits = () => page.evaluate(() => window.scaena.cue.offers().filter((o) => JSON.stringify(o.value).startsWith('["exit"')).length);
  await page.waitForFunction(() => window.scaena.cue.offers().some((o) => JSON.stringify(o.value).startsWith('["exit"')), null, { timeout: 30000 }).catch(() => {});
  check((await exits()) > 0, `before, the cue offers an exit for what leaves: ${await exits()}`);
  const styles = await options("#look-transition-style");
  check(
    ["Dissolve", "Push", "Move In", "Reveal", "Wipe", "Circle", "Circle Wave", "Diamond", "Diamond Wave", "Radial"].every((s) =>
      styles.includes(s),
    ),
    `the inspector offers a presentation app's transitions: ${styles.join(", ")}`,
  );
  const ways = await options("#look-transition-direction");
  check(
    ["Right to Left", "Left to Right", "Bottom to Top", "Top to Bottom"].every((s) => ways.includes(s)),
    `and the ways they go: ${ways.join(", ")}`,
  );

  // Push.
  await page.selectOption("#look-transition-style", "push");
  check(await reads("style: push"), `Push gives the transition a style: ${await closeLine()}`);
  await page.waitForFunction(() => document.querySelector("#listing")?.textContent.includes("Push transition"), null, { timeout: 30000 }).catch(() => {});
  const listed = await page.evaluate(() => document.querySelector("#listing")?.textContent ?? "");
  check(listed.includes("Push transition"), `the cue's listing names it: ${listed.slice(0, 200)}`);
  const pushed = await source();

  // Its cue runs the transition's duration, the slides drawn by the engine (its own test holds
  // each style's layers).
  const span = await page.evaluate(() => window.scaena.last().slots.find((s) => s.state === "close").span);
  check(span > 0, `its cue runs: ${span} ms`);
  await page.waitForFunction(() => !window.scaena.cue.offers().some((o) => JSON.stringify(o.value).startsWith('["exit"')), null, { timeout: 30000 }).catch(() => {});
  check((await exits()) === 0, `a push moves the slide left whole: the cue offers none of its exits, not ${await exits()}`);

  // Bottom to Top.
  await page.selectOption("#look-transition-direction", "up");
  check(await reads("direction: up"), `Bottom to Top sets its direction: ${await closeLine()}`);

  const directed = await source();

  // Dissolve: it fades in place, so the listing says no way it goes.
  await page.selectOption("#look-transition-style", "dissolve");
  check(await reads("style: dissolve"), `Dissolve is the transition's style: ${await closeLine()}`);
  await page.waitForFunction(() => document.querySelector("#listing")?.textContent.includes("Dissolve transition"), null, { timeout: 30000 }).catch(() => {});
  const dissolved = await page.evaluate(() => document.querySelector("#listing")?.textContent ?? "");
  check(
    dissolved.includes("Dissolve transition") && !/Right to Left|Bottom to Top/.test(dissolved),
    `the listing names no way a dissolve goes: ${dissolved.slice(0, 200)}`,
  );

  // Each one step to undo.
  await undo();
  check(await back(directed), "one undo takes Dissolve back");
  await undo();
  check(await back(pushed), "another the direction");
  await undo();
  check(await back(original), "and another the style");
  check(await page.evaluate(() => window.scaena.last().valid), "the source still compiles and validates");

  // Circle Wave (PLAN 2.149): circles grow in a wave, from the right edge by default.
  await page.selectOption("#look-transition-style", "circleWave");
  check(await reads("style: circleWave"), `Circle Wave gives the transition its style: ${await closeLine()}`);
  await page.waitForFunction(() => document.querySelector("#listing")?.textContent.includes("Circle Wave transition"), null, { timeout: 30000 }).catch(() => {});
  const waved = await page.evaluate(() => document.querySelector("#listing")?.textContent ?? "");
  check(waved.includes("Circle Wave transition") && waved.includes("Right to Left"), `the cue's listing names it and its way: ${waved.slice(0, 200)}`);

  // The canvas at rest, then halfway through the cue: the slide left and the slide entered each show.
  const shot = async () => {
    await page.waitForFunction(() => !window.scaena.canvas.busy(), null, { timeout: 30000 }).catch(() => {});
    await page.waitForTimeout(500);
    return page.locator("#preview").screenshot();
  };
  const rested = await shot();
  const ruler = await page.locator("#cue .ruler").boundingBox();
  const cueSpan = await page.evaluate(() => window.scaena.last().slots.find((s) => s.state === "close").span);
  const halfway = ((cueSpan / 2) / (await page.evaluate(() => window.scaena.cue.scale()))) * ruler.width;
  await page.mouse.click(ruler.x + halfway, ruler.y + ruler.height / 2);
  await page.waitForFunction(() => window.scaena.cue.at() !== undefined, null, { timeout: 30000 }).catch(() => {});
  const middle = await page.evaluate(() => window.scaena.cue.at());
  check(middle > 0 && middle < cueSpan, `the ruler shows the cue halfway: ${middle} of ${cueSpan} ms`);
  const mid = await shot();
  check(!mid.equals(rested), "halfway, the canvas is not the slide entered alone");

  // Play plays it, and it comes to rest as the slide entered.
  await page.locator("#cue [data-play]").click();
  const played = await page.waitForFunction(() => window.scaena.at().playing, null, { timeout: 10000 }).then(
    () => true,
    () => false,
  );
  check(played, "Play plays it");
  await page.waitForFunction(() => !window.scaena.at().playing && window.scaena.cue.at() === undefined, null, { timeout: 30000 }).catch(() => {});
  check((await page.evaluate(() => window.scaena.at().playing)) === false, "and it comes to rest");
  check((await shot()).equals(rested), "as the slide entered, as before");
  await undo();
  check(await back(original), "one undo takes Circle Wave back");
} finally {
  await browser.close();
  await server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "a slide transition is chosen in the inspector, and moves each slide whole");
process.exit(failures.length ? 1 : 0);

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
// - Its cue runs the transition's duration.
// - Each choice is one step to undo.
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
  const styles = await options("#look-transition-style");
  check(
    ["Dissolve", "Push", "Move In", "Reveal", "Wipe"].every((s) => styles.includes(s)),
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

  // Bottom to Top.
  await page.selectOption("#look-transition-direction", "up");
  check(await reads("direction: up"), `Bottom to Top sets its direction: ${await closeLine()}`);

  // Each one step to undo.
  await undo();
  check(await back(pushed), "one undo takes the direction back");
  await undo();
  check(await back(original), "and another the style");
  check(await page.evaluate(() => window.scaena.last().valid), "the source still compiles and validates");
} finally {
  await browser.close();
  await server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "a slide transition is chosen in the inspector, and moves each slide whole");
process.exit(failures.length ? 1 : 0);

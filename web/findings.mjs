// PLAN 2.49 check: lint's findings on the canvas, in headless Chromium (serve.mjs), the CPU
// painting, on the revenue example, in `revenue`.
//
//   node web/findings.mjs     (after `just web`; from the repository's root)
//
// - The example lints clean: no mark on the canvas, no count in the strip.
// - A headline too long for its header is an error mark at the title's box's top right corner,
//   and the strip counts it on `revenue` alone. Opened, the mark says E100 and its hint and offers
//   its fix; the fix taken is one patch (`fit:shrink`), and the mark goes. One undo brings it back.
// - Forty-five words on screen are W210, about the state: its mark stands at the canvas's top left
//   corner, and opened, it goes to where the source writes the state.
// - Zoomed in where the title is not, its mark is not shown; the whole canvas again, it is.
// - In 9:16, the marks and the counts are what lint found laying 9:16 out, and the document
//   rules'; on the deck's own canvas again, its own.
// - axe-core finds nothing against WCAG 2.1 AA in the marks, the strip, or a mark opened.
// - In the haiku example, a poem of 5, 8, and 5 is I404, said with its counts, the syllable table
//   handed to the editor's module (PLAN O.4h). A table that never comes is given up after the
//   worker's wait, and the editor goes on (lift-up REL-3).
// Exits 1 on any failure.
import { axeInto, launch, serve } from "./serve.mjs";

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
  await page.goto(`${server.origin}/web/dist/editor.html?painter=cpu&source=open&bundle=/docs/examples/revenue.deck.json`);
  await page.waitForFunction(() => window.scaena?.last(), null, { timeout: 120000 });

  const source = () => page.evaluate(() => window.scaena.source());
  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  const marks = () => page.evaluate(() => window.scaena.canvas.marks());
  const mark = async (node) => (await marks()).find((m) => m.node === node);
  /** The strip's count on `state`: its severity and number, if it has one. */
  const counted = (state) =>
    page.evaluate((s) => {
      const found = document.querySelector(`#strip li[data-state="${s}"] .found`);
      return found ? { severity: ["error", "warning", "info"].find((c) => found.classList.contains(c)), count: Number(found.firstChild.textContent) } : undefined;
    }, state);
  const open = () => page.evaluate(() => document.querySelector("#marked").matches(":popover-open"));
  const popped = () => page.evaluate(() => document.querySelector("#marked").textContent);
  /** Once `test` holds of the marks (polled in the page), and the canvas has caught up. */
  const until = async (test, arg) => {
    const ok = await page
      .waitForFunction(test, arg, { timeout: 30000, polling: 50 })
      .then(() => true, () => false);
    await page.waitForTimeout(200);
    return ok;
  };
  const says = async (text) => {
    await page.waitForFunction((t) => document.querySelector("#status").textContent.includes(t), text, { timeout: 30000 }).catch(() => {});
    return (await status()).includes(text);
  };
  /** `revenue` shown, the cursor on its declaration. */
  const inRevenue = async () => {
    await page.evaluate(() => window.scaena.cursor(window.scaena.source().indexOf("state revenue") + "state ".length));
    await page.waitForFunction(() => window.scaena.shown() === 1 && window.scaena.canvas.boxed() === "revenue", null, { timeout: 30000 });
  };
  /** The source as `to`, one edit, in `revenue`; once it is linted, every state too. */
  const type = async (to) => {
    await page.evaluate((s) => window.scaena.type(s), to);
    await page.waitForFunction((s) => window.scaena.source() === s && window.scaena.last()?.whole, to, { timeout: 60000, polling: 50 }).catch(() => {});
    await inRevenue();
    await page.waitForTimeout(300);
  };
  /** Where `node`'s box has its top right corner, CSS px in the marks' layer. */
  const corner = (node) =>
    page.evaluate((n) => {
      const r = document.querySelector("#marks").getBoundingClientRect();
      const [vx, vy, vw, vh] = window.scaena.canvas.view();
      const [x, y, w] = window.scaena.canvas.boxes().find((b) => b.node === n).rect;
      return [((x + w - vx) / vw) * r.width, ((y - vy) / vh) * r.height];
    }, node);
  /** What axe-core finds against WCAG 2.1 A and AA in `selectors`. */
  const audit = async (what, ...selectors) => {
    await axeInto(page);
    const found = await page.evaluate(async (include) => {
      const result = await window.axe.run({ include }, { runOnly: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"] });
      return result.violations.map((v) => `${v.id} (${v.impact}): ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
    }, selectors.map((s) => [s]));
    check(!found.length, `${what}: axe finds nothing against WCAG 2.1 AA${found.length ? `:\n    ${found.join("\n    ")}` : ""}`);
  };

  // In `revenue`, the slide the example's chart is on.
  await inRevenue();
  await page.waitForFunction(() => window.scaena.last()?.whole, null, { timeout: 60000 });
  const original = await source();
  check((await marks()).length === 0, `the example lints clean: no mark (${JSON.stringify(await marks())})`);
  check((await page.locator("#strip .found").count()) === 0, "and no count in the strip");

  // A headline too long for its header: E100 on the title, an error mark at its box's corner.
  const long = original.replace('"Revenue doubled"', '"Revenue doubled, and then some, and more"');
  await type(long);
  check(await until(() => window.scaena.canvas.marks().some((m) => m.node === "title" && m.severity === "error")), "a headline too long: an error mark on the title");
  const title = await mark("title");
  const [cx, cy] = await corner("title");
  check(Boolean(title) && Math.hypot(title.at[0] - cx, title.at[1] - cy) < 1.5, `at its box's top right corner: ${title?.at} by ${[cx, cy]}`);
  const own = await page.evaluate(() => window.scaena.last().findings.filter((f) => f.state === "revenue" && f.shown && f.node === "title").length);
  check(title?.count === own, `it counts the title's findings: ${title?.count} of ${own}`);
  const counts = await counted("revenue");
  check(counts?.severity === "error" && counts.count >= own, `the strip counts them on revenue: ${JSON.stringify(counts)}`);
  check((await counted("intro")) === undefined && (await counted("close")) === undefined, "and on no state the headline is not in");

  // Opened: what it found, its hint, and its fix, which is one patch and one undo.
  await page.locator('#marks [data-mark="title"]').click();
  check(await until(() => document.querySelector("#marked").matches(":popover-open")), "a mark opened shows its findings");
  // In a sentence, as the Mac says it (PLAN 2.99): no code.
  check(
    (await popped()).includes("The text doesn't fit its box.") && !(await popped()).includes("E100"),
    `it says E100 in a sentence: ${(await popped()).slice(0, 120)}`,
  );
  check(await page.evaluate(() => window.scaena.canvas.selected() === "title"), "and selects the title");
  await audit("a mark opened", "#marks", "#marked", "#strip");
  const fix = page.locator('#marked [data-fix]').first();
  check((await fix.textContent()) === "Shrink to Fit", `it offers the fix lint checked, by name: ${await fix.textContent()}`);
  await fix.click();
  check(await until(() => window.scaena.source().includes("fit:shrink")), "the fix taken: the title shrinks to fit");
  check(await says("Fixed “Revenue doubled"), `the status says so: ${await status()}`);
  check(await until(() => !window.scaena.canvas.marks().some((m) => m.node === "title" && m.severity === "error")), `and its error mark is gone: ${JSON.stringify(await marks())}`);
  check(!(await open()), "the popover closed");
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+z");
  check(await until((s) => window.scaena.source() === s, long), "one undo takes the fix back");
  check(await until(() => window.scaena.canvas.marks().some((m) => m.node === "title" && m.severity === "error")), "and its mark with it");

  // Forty-five words on screen: W210, about the state, on the state's mark.
  await type(original);
  const words = Array.from({ length: 44 }, (_, i) => ["revenue", "grew", "in", "every", "segment", "this", "quarter"][i % 7]).join(" ");
  const dense = original.replace('"Revenue in $M. Enterprise recognized on delivery."', `"${words}."`);
  await type(dense);
  check(await until(() => window.scaena.canvas.marks().some((m) => m.node === null)), `W210 stands on the state: ${JSON.stringify(await marks())}`);
  const state = await mark(null);
  check(Boolean(state) && state.at[0] < 20 && state.at[1] < 20, `at the canvas's top left corner: ${state?.at}`);
  const about = await page.evaluate(() => window.scaena.last().findings.filter((f) => f.state === "revenue" && f.shown && f.code === "W210").length);
  check(about === 1 && state?.severity === "warning", `a warning: ${state?.severity}`);
  await page.locator('#marks [data-mark=""]').click();
  check(
    await until(() => (document.querySelector("#marked").textContent ?? "").includes("This slide has more words than the theme allows.")),
    `opened, it says W210 in a sentence: ${(await popped()).slice(0, 120)}`,
  );
  check((await page.locator("#marked [data-fix]").count()) === 0, "W210 has no fix, and none is offered");
  await page.locator("#marked [data-go]").first().click();
  check(await until(() => document.activeElement?.closest(".cm-content") !== null && !document.querySelector("#marked").matches(":popover-open")), "it goes to the source");
  const line = await page.evaluate(() => document.querySelector(".cm-activeLine")?.textContent ?? "");
  check(line.startsWith("state revenue"), `where the source writes the state: ${line}`);

  // Zoomed in where the title is not, its mark is not shown.
  await type(long);
  await until(() => window.scaena.canvas.marks().some((m) => m.node === "title"));
  await page.evaluate(() => window.scaena.canvas.zoom(4, [1400, 800]));
  check(await until(() => !window.scaena.canvas.marks().some((m) => m.node === "title")), `zoomed in elsewhere: no mark for the title (${JSON.stringify(await marks())})`);
  await page.evaluate(() => window.scaena.canvas.zoom("fit"));
  check(await until(() => window.scaena.canvas.marks().some((m) => m.node === "title")), "the whole canvas again: its mark is back");

  // In 9:16: what lint found laying 9:16 out, and what holds in every format.
  await page.selectOption("#format", "9:16");
  check(
    await until(() => {
      const f = window.scaena.last().findings;
      return f.some((g) => g.format === "9:16" && g.shown) && !f.some((g) => g.format === undefined && g.code === "E100" && g.shown);
    }),
    "in 9:16, lint says again what holds there",
  );
  const tall = await page.evaluate(() => window.scaena.last().findings.filter((f) => f.state === "revenue" && f.shown));
  check(tall.length > 0 && tall.every((f) => f.format === "9:16" || ["W301", "W302"].includes(f.code) || !["E100", "E101", "W202"].includes(f.code)), `what it shows: ${tall.map((f) => `${f.code}${f.format ? `@${f.format}` : ""}`)}`);
  check(await until((n) => document.querySelector('#strip li[data-state="revenue"] .found')?.firstChild?.textContent === String(n), tall.length), `the strip counts them: ${JSON.stringify(await counted("revenue"))} of ${tall.length}`);
  check(await until(() => window.scaena.canvas.marks().some((m) => m.node === "title")), "and the title's mark stands on its box in 9:16");
  const [tx, ty] = await corner("title");
  const tallTitle = await mark("title");
  check(Boolean(tallTitle) && Math.hypot(tallTitle.at[0] - tx, tallTitle.at[1] - ty) < 1.5, `at its corner there: ${tallTitle?.at} by ${[tx, ty]}`);
  await page.selectOption("#format", "");
  check(
    await until(() => window.scaena.last().findings.some((g) => g.format === undefined && g.code === "E100" && g.shown)),
    "on the deck's own canvas again, its own",
  );
  check(await page.evaluate(() => window.scaena.last().valid), "the source compiles and validates");

  // A haiku off 5-7-5 (PLAN O.4h): the worker counts it with the syllable table it is handed the
  // first time a line counts, and lint marks the poem with its counts in a sentence.
  await page.goto(`${server.origin}/web/dist/editor.html?painter=cpu&source=open&bundle=/docs/examples/haiku.deck.json`);
  await page.waitForFunction(() => window.scaena?.last()?.whole, null, { timeout: 120000 });
  const haiku = await source();
  check(!(await page.evaluate(() => window.scaena.last().findings.some((f) => f.code === "I404"))), "a haiku of 5, 7, and 5 is not marked");
  await page.evaluate((s) => window.scaena.type(s), haiku.replace("A frog jumps", "A green frog jumps"));
  check(
    await until(() => window.scaena.last().findings.some((f) => f.code === "I404" && f.node === "poem" && f.said === "This haiku's lines count 5, 8, and 5, not 5, 7, and 5.")),
    "one of 5, 8, and 5 is, with its counts, the table handed over",
  );
  check(await until(() => window.scaena.canvas.marks().some((m) => m.node === "poem")), "and marked on the poem");

  // A table that never comes (lift-up REL-3): the worker's request for it gives up after its
  // wait, and the engine goes on, linting the poem with no count, where it would hold every
  // edit, frame, and lint behind it for good.
  const stalled = await browser.newContext({ viewport: { width: 1920, height: 1200 }, deviceScaleFactor: 1 });
  let asked = 0;
  await stalled.route("**/words-*.txt", () => {
    asked++;
  });
  const waiting = await stalled.newPage();
  waiting.on("pageerror", (e) => failures.push(`page: ${e.message}`));
  const started = Date.now();
  await waiting.goto(`${server.origin}/web/dist/editor.html?painter=cpu&source=open&bundle=/docs/examples/haiku.deck.json`);
  await waiting.waitForFunction(() => window.scaena?.last()?.whole, null, { timeout: 120000 });
  const waited = Date.now() - started;
  check(asked > 0, `the syllable table was asked for (${asked} times)`);
  // Each request gives up after the worker's 10 s; the engine is held no longer than they add up to.
  check(waited >= 9000 && waited < asked * 10000 + 30000, `the engine waited for it, each request no longer than its wait (${waited} ms, ${asked} requests)`);
  check(!(await waiting.evaluate(() => window.scaena.last().findings.some((f) => f.code === "I404"))), "and lints the poem with no count");
  await waiting.evaluate((s) => window.scaena.type(s), haiku.replace("A frog jumps", "A green frog jumps"));
  check(
    await waiting.waitForFunction((s) => window.scaena.source().includes("A green frog jumps") && window.scaena.last()?.valid, null, { timeout: 60000 }).then(() => true, () => false),
    "an edit after it is compiled and linted",
  );
  await stalled.close();
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

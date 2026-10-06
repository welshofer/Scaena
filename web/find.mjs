// PLAN 2.47 check: find and replace across the deck's texts, in headless Chromium (serve.mjs), the
// CPU painting, on the revenue example.
//
//   node web/find.mjs     (after `just web`; from the repository's root)
//
// - ⌘F on the canvas opens the find bar; what is typed is found in every state's texts, once for
//   each place a text is written: the title's own, `revenue`'s delta, and the note's own.
// - Enter goes to the next match, Shift+Enter to the one before: its state shown, its node
//   selected, its characters marked on the canvas.
// - Replace replaces the match shown, one patch, where its text lives, and shows the next after
//   what it put in, which the query may still find; one undo takes it back.
// - Replace All replaces every match in one patch, one step to undo.
// - Match case and Whole words, as asked; Escape closes the bar and takes the mark away.
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
  const trips = () => page.evaluate(() => window.scaena.trips().length);
  const settled = (before) =>
    page.waitForFunction((n) => window.scaena.trips().length > n && !window.scaena.canvas.busy(), before, { timeout: 60000, polling: 50 });
  const says = () => page.evaluate(() => document.querySelector("#find output").value);
  const saying = (what) =>
    page.waitForFunction((w) => document.querySelector("#find output").value === w, what, { timeout: 30000 }).then(
      () => true,
      () => false,
    );
  const shownState = () => page.evaluate(() => window.scaena.last().states[window.scaena.shown()][0]);
  const selected = () => page.evaluate(() => window.scaena.canvas.selected());
  const marks = () => page.evaluate(() => document.querySelectorAll("#overlay svg rect.found").length);
  const original = await source();
  await page.waitForFunction(() => window.scaena.canvas.boxes().length > 0, null, { timeout: 30000 });

  // ⌘F on the canvas opens the bar, the find field focused.
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+f");
  await page.waitForFunction(() => !document.querySelector("#find").hidden, null, { timeout: 10000 }).catch(() => {});
  check(await page.evaluate(() => !document.querySelector("#find").hidden && document.activeElement?.getAttribute("name") === "find"), "⌘F opens the find bar, its field focused");

  // Found once for each place a text is written: the title's own, `revenue`'s, and the note's.
  await page.keyboard.type("rev");
  check(await saying("3 matches"), `“rev” is 3 matches: ${await says()}`);
  const found = await page.evaluate(() => window.scaena.find.found().map((f) => `${f.node} ${f.lives}`));
  check(
    found.join(" | ") === "title /nodes/title/text | title /states/1/props/title/text | note /nodes/note/text",
    `each where it is written: ${found.join(" | ")}`,
  );

  // Enter goes to each match in turn: its state, its node, its characters marked.
  await page.keyboard.press("Enter");
  check(await saying("1 of 3"), `Enter: ${await says()}`);
  await page.waitForFunction(() => document.querySelectorAll("#overlay svg rect.found").length > 0, null, { timeout: 30000 }).catch(() => {});
  check((await shownState()) === "intro" && (await selected()) === "title" && (await marks()) > 0, `the first in intro's title, marked: ${await shownState()}, ${await selected()}, ${await marks()}`);
  await page.keyboard.press("Enter");
  check(await saying("2 of 3"), `Enter again: ${await says()}`);
  await page.waitForFunction(() => window.scaena.last().states[window.scaena.shown()][0] === "revenue", null, { timeout: 30000 }).catch(() => {});
  check((await shownState()) === "revenue" && (await selected()) === "title", `the second in revenue's title: ${await shownState()}, ${await selected()}`);
  await page.keyboard.press("Shift+Enter");
  check(await saying("1 of 3"), `Shift+Enter goes back: ${await says()}`);
  check(await page.evaluate(() => document.activeElement?.getAttribute("name") === "find"), "and the find field keeps the keys");

  // Replace: the match shown, one patch, where its text lives; then the next after what was put in
  // its place, which the query, case alike, still finds.
  await page.locator('#find input[name="replace"]').fill("REV");
  let n = await trips();
  await page.locator('#find [data-find="replace"]').click();
  await settled(n);
  check((await source()).includes("Q3 REView") && !(await source()).includes("REVenue"), "Replace changes the match shown, and only it");
  check(await saying("2 of 3"), `the next shown after it, not the one put in: ${await says()}`);
  await page.waitForFunction(() => window.scaena.last().states[window.scaena.shown()][0] === "revenue", null, { timeout: 30000 }).catch(() => {});
  check((await shownState()) === "revenue", `in its state: ${await shownState()}`);
  await page.locator("#overlay").focus();
  n = await trips();
  await page.keyboard.press("Control+z");
  await settled(n);
  check((await source()) === original, "one undo takes it back");

  // Replace All: every match of a whole word, one patch, one step to undo.
  await page.keyboard.press("Control+f");
  await page.locator('#find input[name="find"]').fill("Revenue");
  await page.locator('#find input[name="words"]').check();
  check(await saying("2 matches"), `“Revenue”, whole words: ${await says()}`);
  await page.locator('#find input[name="replace"]').fill("Income");
  n = await trips();
  await page.locator('#find [data-find="all"]').click();
  await settled(n);
  // The slides' texts, that is; the spine's claims and scripts are no text a slide shows.
  const all = await source();
  check(
    all.includes('title "Income doubled"') && all.includes('"Income in $M.') && !all.includes('title "Revenue doubled"') && !all.includes('"Revenue in $M.'),
    "Replace All replaces every match",
  );
  check(all.includes('beat doubled "Revenue doubled year over year'), "and leaves the spine's claims as they were");
  check(await saying("no match"), `and none is left: ${await says()}`);
  check((await trips()) === n + 1, "in one patch");
  check(await page.evaluate(() => window.scaena.last().valid), "the deck validates");
  await page.locator("#overlay").focus();
  n = await trips();
  await page.keyboard.press("Control+z");
  await settled(n);
  check((await source()) === original, "one undo takes all of it back");

  // Case apart: none of the deck's texts says “revenue” in lower case.
  await page.keyboard.press("Control+f");
  await page.locator('#find input[name="words"]').uncheck();
  await page.locator('#find input[name="find"]').fill("revenue");
  await page.locator('#find input[name="case"]').check();
  check(await saying("no match"), `“revenue”, case apart: ${await says()}`);
  await page.locator('#find input[name="case"]').uncheck();
  check(await saying("2 matches"), `and alike: ${await says()}`);

  // Escape closes the bar, and the mark goes.
  await page.keyboard.press("Enter");
  await page.waitForFunction(() => document.querySelectorAll("#overlay svg rect.found").length > 0, null, { timeout: 30000 }).catch(() => {});
  await page.keyboard.press("Escape");
  await page.waitForFunction(() => document.querySelector("#find").hidden, null, { timeout: 10000 }).catch(() => {});
  check(await page.evaluate(() => document.querySelector("#find").hidden), "Escape closes the bar");
  check((await marks()) === 0, "and takes the mark away");
  check((await source()) === original, "the deck is as it was");
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

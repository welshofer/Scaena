// PLAN 2.43 check: group and ungroup on the editor's canvas, in headless Chromium (serve.mjs), the
// CPU painting, as in web/canvas.mjs.
//
//   node web/group.mjs     (after `just web`; from the repository's root)
//
// On the torture deck's `containers` case:
// - The tally and the card selected, ⌘G puts them in a new group, `group`, where they stand: one
//   patch, the group selected, nothing moved; one undo takes it back.
// - ⌘⇧G on the group takes it apart: the deck is as it was, the tally and the card selected.
// - The inspector's Group and Ungroup buttons do the same.
// - A stat's figure and label, which a stack holds, are refused: a stack places what it holds.
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
  await page.goto(`${server.origin}/web/dist/editor.html?painter=cpu&bundle=/tests/fixtures/torture.scaena/deck.json`);
  await page.waitForFunction(() => window.scaena?.last(), null, { timeout: 120000 });

  const source = () => page.evaluate(() => window.scaena.source());
  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  const chosen = () => page.evaluate(() => window.scaena.canvas.chosen());
  const trips = () => page.evaluate(() => window.scaena.trips().length);
  /** Once lint has answered for the source the editor holds after `before` trips. */
  const settled = (before) =>
    page.waitForFunction((n) => window.scaena.trips().length > n && !window.scaena.canvas.busy(), before, { timeout: 60000, polling: 50 });
  /** Once the status says `what`. */
  const says = (what) =>
    page.waitForFunction((w) => document.querySelector("#status").textContent.includes(w), what, { timeout: 30000 }).then(
      () => true,
      () => false,
    );
  /** Where each node that draws stands, by node: what a group must not change. */
  const boxes = () =>
    page.evaluate(() =>
      JSON.stringify(
        window.scaena.canvas
          .boxes()
          .filter((b) => b.draws)
          .map((b) => [b.node, b.rect])
          .sort((a, b) => (a[0] < b[0] ? -1 : 1)),
      ),
    );
  /** `node`'s declaration in the source: its first line and those after it set deeper. */
  const declared = (scn, node) => {
    const lines = scn.split("\n");
    const first = lines.findIndex((l) => l.trim() === node || l.trim().startsWith(`${node} `));
    if (first < 0) return "";
    const deeper = (l) => l.search(/\S/) > lines[first].search(/\S/);
    let last = first + 1;
    while (last < lines.length && deeper(lines[last])) last++;
    return lines.slice(first, last).join(" ");
  };
  /** Press `keys` on the canvas, focused, and wait for the status to say `done`. */
  const press = async (keys, done) => {
    const n = await trips();
    await page.locator("#overlay").focus();
    await page.keyboard.press(keys);
    await settled(n);
    return says(done);
  };

  const index = await page.evaluate(() => window.scaena.opened.states.indexOf("containers"));
  const at = (await source()).indexOf("state containers") + "state ".length;
  await page.evaluate((offset) => window.scaena.cursor(offset), at);
  await page.waitForFunction((i) => window.scaena.shown() === i && window.scaena.at().index === i, index, { timeout: 30000 });
  await page.waitForFunction(() => window.scaena.canvas.boxed() === "containers" && window.scaena.canvas.boxes().length > 0, null, { timeout: 30000 });
  const original = await source();
  const standing = await boxes();
  /** One step back, to the source as the test found it, and the canvas standing so again. */
  const undo = async () => {
    await press("Control+z", "");
    await page
      .waitForFunction(
        (b) =>
          JSON.stringify(
            window.scaena.canvas
              .boxes()
              .filter((x) => x.draws)
              .map((x) => [x.node, x.rect])
              .sort((p, q) => (p[0] < q[0] ? -1 : 1)),
          ) === b,
        standing,
        { timeout: 30000 },
      )
      .catch(() => {});
  };

  // ⌘G: the tally and the card in a new group, where they stand.
  await page.evaluate(() => window.scaena.canvas.selectAll(["tally", "card"]));
  check(await press("Control+g", "tally, card grouped as group"), `⌘G groups them: ${await status()}`);
  const grouped = await source();
  // A node whose id is its type is written by its id alone.
  check(declared(grouped, "group").trim() === "group", `the source declares the group: ${declared(grouped, "group").trim()}`);
  check(/\bparent\(group\)/.test(declared(grouped, "tally")) && /\bparent\(group\)/.test(declared(grouped, "card")), `each is in it: ${declared(grouped, "tally").trim()}`);
  check((await chosen()).join() === "group", `the group is selected: ${await chosen()}`);
  check((await boxes()) === standing, "nothing moved");
  await undo();
  check((await source()) === original, "one undo takes the group away");

  // ⌘⇧G: the group taken apart, the deck as it was.
  await page.evaluate(() => window.scaena.canvas.selectAll(["tally", "card"]));
  check(await press("Control+g", "grouped as group"), "grouped again");
  await page.waitForFunction(() => window.scaena.canvas.chosen().join() === "group", null, { timeout: 30000 }).catch(() => {});
  check(await press("Control+Shift+g", "group taken apart"), `⌘⇧G takes it apart: ${await status()}`);
  check((await source()) === original, "and the deck is as it was");
  check(JSON.stringify((await chosen()).sort()) === JSON.stringify(["card", "tally"]), `its children are selected: ${await chosen()}`);

  // The inspector's Group and Ungroup.
  await page.waitForFunction(() => document.querySelector("#look [data-group]"), null, { timeout: 30000 }).catch(() => {});
  let n = await trips();
  await page.locator("#look [data-group]").click();
  await settled(n);
  check(await says("grouped as group"), `the inspector's Group groups them: ${await status()}`);
  await page.waitForFunction(() => document.querySelector("#look [data-ungroup]"), null, { timeout: 30000 }).catch(() => {});
  check((await page.locator("#look [data-ungroup]").count()) === 1, "with the group selected, it offers Ungroup");
  n = await trips();
  await page.locator("#look [data-ungroup]").click();
  await settled(n);
  check(await says("group taken apart"), `and Ungroup takes it apart: ${await status()}`);
  check((await source()) === original, "the deck as it was");

  // A stack places what it holds: its children are not grouped.
  await page.evaluate(() => window.scaena.canvas.selectAll(["stat-a-figure", "stat-a-label"]));
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+g");
  check(await says("places what it holds itself"), `a stack's children are refused, with why: ${await status()}`);
  check((await source()) === original, "and nothing changes");
  check(await page.evaluate(() => window.scaena.last().valid), "the torture deck still validates");
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

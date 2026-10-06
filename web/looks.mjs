// PLAN 2.58 check: a look copied and pasted on the editor's canvas, in headless Chromium
// (serve.mjs), the CPU painting.
//
//   node web/looks.mjs     (after `just web`; from the repository's root)
//
// On the revenue example, in `revenue`:
// - ⌥⌘C (Ctrl+Alt+C) copies the look of the node selected: the title's role there, `headline`,
//   the rest of its look the theme's; the status says what it sets.
// - ⌥⌘V (Ctrl+Alt+V) pastes it on the note: one `choose`, written where the note's role lives,
//   on the node; one undo takes it back. Pasted on a note that looks so already, it changes
//   nothing, and says so.
// - On the note and the chart together, the note takes it, and the status says the chart takes
//   none of a text's look.
// - The palette offers both, with their keys: Paste the look only once a look is copied.
// On the torture deck:
// - In `shapes`, the pill's look pasted on the ring: the pill's fill and corners, and the ring's
//   stroke taken away, as the pill's is the theme's.
// - In `containers`, a shape's look on an image and a stack: the image takes its corners, and the
//   stack none of it.
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

  const source = () => page.evaluate(() => window.scaena.source());
  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  /** Open `bundle` in the editor, showing the state declared as `state`. */
  const open = async (bundle, state) => {
    await page.goto(url(bundle));
    await page.waitForFunction(() => window.scaena?.last(), null, { timeout: 120000 });
    await into(state);
  };
  /** Show the state declared as `state` in the source, and what stands where in it. */
  const into = async (state) => {
    await page.evaluate((s) => window.scaena.cursor(window.scaena.source().indexOf(`state ${s}`) + "state ".length), state);
    await page.waitForFunction((s) => window.scaena.canvas.boxed() === s && window.scaena.canvas.boxes().length > 0, state, { timeout: 30000 });
  };
  /** Select `nodes` on the canvas and press `keys` there; what the status says once it says
   * `says`. */
  const press = async (nodes, keys, says) => {
    await page.evaluate((ns) => (ns.length > 1 ? window.scaena.canvas.selectAll(ns) : window.scaena.canvas.select(ns[0])), nodes);
    await page.locator("#overlay").focus();
    await page.keyboard.press(keys);
    await page.waitForFunction((s) => document.querySelector("#status").textContent.includes(s), says, { timeout: 30000 }).catch(() => {});
    return status();
  };
  const undo = async (to) => {
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+z");
    await page.waitForFunction((s) => window.scaena.source() === s, to, { timeout: 30000, polling: 50 }).catch(() => {});
    return (await source()) === to;
  };
  /** What the inspector shows of `node`'s look, inspected afresh: each property the look takes,
   * and its value. */
  const shows = async (node, props) => {
    await page.evaluate(() => window.scaena.canvas.select(undefined));
    await page.waitForFunction((n) => window.scaena.look.offered()?.node !== n, node, { timeout: 30000 });
    await page.evaluate((n) => window.scaena.canvas.select(n), node);
    await page.waitForFunction((n) => window.scaena.look.offered()?.node === n, node, { timeout: 30000 });
    await page.evaluate(() => window.scaena.look.settled());
    return page.evaluate(
      (ps) => Object.fromEntries(ps.map((p) => [p, window.scaena.look.offered()?.fields.find((f) => f.prop === p)?.value ?? null])),
      props,
    );
  };
  /** What the palette offers for `query`: the first command's label and its key. */
  const offers = async (query) => {
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+k");
    await page.waitForFunction(() => document.querySelector("#palette").open, null, { timeout: 10000 });
    await page.keyboard.type(query);
    const label = (await page.evaluate(() => window.scaena.commands.shown()))[0];
    const key = await page.evaluate(() => document.querySelector("#palette [role=option] kbd")?.textContent ?? "");
    await page.keyboard.press("Escape");
    return [label, key];
  };

  await open("/docs/examples/revenue.deck.json", "revenue");
  const original = await source();

  // Nothing is copied yet: the palette does not offer to paste one.
  await page.evaluate(() => window.scaena.canvas.select("note"));
  const [none] = await offers("paste the look");
  check(none !== "Paste the look", `with no look copied, the palette offers no paste: ${none}`);

  // ⌥⌘C on the title: its role in `revenue`, and the theme's for the rest.
  const copied = await press(["title"], "Control+Alt+KeyC", "look copied");
  check(copied.includes("title's look copied (role headline)"), `⌥⌘C copies the title's look: ${copied}`);
  const look = await page.evaluate(() => window.scaena.canvas.copiedLook());
  check(
    look?.node === "title" && look.props[0]?.prop === "role" && look.props[0]?.value === "headline" && look.props.slice(1).every((p) => p.value === undefined),
    `a text's look is its role and its style, as the state shows them: ${JSON.stringify(look?.props)}`,
  );
  check((await source()) === original, "copying a look changes nothing");

  // ⌥⌘V on the note: its role lives on the node, so the look goes there.
  const pasted = await press(["note"], "Control+Alt+KeyV", "look pasted");
  check(pasted.includes("title's look pasted on note"), `⌥⌘V pastes it on the note: ${pasted}`);
  check((await source()).includes('note text role:headline "Revenue in $M.'), "written where the note's role lives, on the node");
  check(await page.evaluate(() => window.scaena.canvas.chosen().join()) === "note", "the note stays selected");
  const again = await press(["note"], "Control+Alt+KeyV", "so already");
  check(again.includes("note looks so already"), `pasted again, it changes nothing, and says so: ${again}`);
  check(await undo(original), "one undo takes it back");

  // On the note and the chart together: the note takes it, and the chart's refusal is said.
  const both = await press(["note", "rev"], "Control+Alt+KeyV", "look pasted");
  check(
    both.includes("title's look pasted on note") && both.includes("rev: a chart takes none of a text's look"),
    `on several, each that takes it, and what the others cannot: ${both}`,
  );
  check(await undo(original), "and one undo takes that back");

  // The palette offers both, with their keys.
  await page.evaluate(() => window.scaena.canvas.select("note"));
  const [copyLabel, copyKey] = await offers("copy the look");
  check(copyLabel === "Copy the look" && copyKey === "Ctrl+Alt+C", `the palette offers Copy the look: ${copyLabel}, ${copyKey}`);
  const [pasteLabel, pasteKey] = await offers("paste the look");
  check(pasteLabel === "Paste the look" && pasteKey === "Ctrl+Alt+V", `and Paste the look, once one is copied: ${pasteLabel}, ${pasteKey}`);

  // A shape's look: its fill, stroke, and corners. The pill has a fill and no stroke of its own;
  // on the ring, its fill and corners go down, and the ring's stroke is taken away.
  await open("/tests/fixtures/torture.scaena/deck.json", "shapes");
  const props = ["fill", "stroke/paint", "stroke/width", "radius"];
  const ring = await shows("shape-ring", props);
  check(ring.fill === null && ring["stroke/paint"] === "accent" && ring["stroke/width"] === "thick", `the ring is a stroke: ${JSON.stringify(ring)}`);
  await press(["shape-pill"], "Control+Alt+KeyC", "look copied");
  const onRing = await press(["shape-ring"], "Control+Alt+KeyV", "look pasted");
  check(onRing.includes("shape-pill's look pasted on shape-ring"), `the pill's look on the ring: ${onRing}`);
  const filled = await shows("shape-ring", props);
  check(
    filled.fill === "accent" && filled.radius === "radius.5" && filled["stroke/paint"] === null && filled["stroke/width"] === null,
    `it takes the pill's fill and corners, and the ring's stroke is taken away: ${JSON.stringify(filled)}`,
  );

  // On an image, a shape's corners alone; a stack takes none of it.
  await into("containers");
  await press(["card-tag"], "Control+Alt+KeyC", "look copied");
  const photo = await press(["board-photo"], "Control+Alt+KeyV", "look pasted");
  check(photo.includes("card-tag's look pasted on board-photo"), `a shape's look on an image: ${photo}`);
  const corners = await shows("board-photo", ["radius", "fit"]);
  check(corners.radius === "radius.5", `the image takes its corners: ${JSON.stringify(corners)}`);
  const stack = await press(["stats"], "Control+Alt+KeyV", "takes none");
  check(stack.includes("stats: a stack takes none of a shape's look"), `a stack takes none of it, and says so: ${stack}`);
} catch (e) {
  failures.push(String(e));
} finally {
  await browser.close();
  server.close();
}

if (failures.length) {
  console.error(`\n${failures.length} failed:\n${failures.map((f) => `  ${f}`).join("\n")}`);
  process.exit(1);
}
console.log("\nall looks checks pass");

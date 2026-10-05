// PLAN 2.42 check: several nodes at once on the editor's canvas, in headless Chromium (serve.mjs),
// the CPU painting, as in web/canvas.mjs.
//
//   node web/several.mjs     (after `just web`; from the repository's root)
//
// On the torture deck's `containers` case:
// - Shift+click puts the card beside the tally that is selected: both are outlined, without
//   handles, and the inspector says what is selected and offers to arrange it.
// - A drag of the card moves both a row up: one patch, one step to undo.
// - The inspector's Left aligns the card on the tally's left edge: one patch.
// - ⌘⇧] puts the card's photo in front of everything the card holds: its `z`.
// - A drag across empty canvas selects the children of the canvas it encloses, the tally and
//   the marks; Delete takes both out of the state, one patch, and one undo puts them back.
// - ⌘C on the tally and the card puts one clip of both on the clipboard; ⌘V pastes both where
//   the canvas was last pressed, as they stood about each other, both selected; ⌘X takes both
//   out. Each is one patch, one step to undo.
// - With a stat's figure and label selected, the inspector offers what they share: a role chosen
//   there is both of theirs, one patch.
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
  const box = (node) => page.evaluate((n) => window.scaena.canvas.boxes().find((b) => b.node === n)?.rect, node);
  const center = ([x, y, w, h]) => [x + w / 2, y + h / 2];
  /** A point in canvas units, on the page. */
  const client = ([x, y]) =>
    page.evaluate(
      ([x, y]) => {
        const r = document.querySelector("#overlay").getBoundingClientRect();
        const [w, h] = window.scaena.canvas.size();
        return [r.left + (x / w) * r.width, r.top + (y / h) * r.height];
      },
      [x, y],
    );
  const click = async (at, modifiers = []) => {
    const [x, y] = await client(at);
    for (const key of modifiers) await page.keyboard.down(key);
    await page.mouse.click(x, y);
    for (const key of modifiers) await page.keyboard.up(key);
  };
  /** How the inspector's listing says `node` is placed, once it says `expected`. */
  const placedAs = async (node, expected) => {
    const read = (n) => document.querySelector(`#inspector tr[data-node="${n}"]`)?.children[1]?.textContent ?? "";
    await page
      .waitForFunction(([n, e]) => (document.querySelector(`#inspector tr[data-node="${n}"]`)?.children[1]?.textContent ?? "").includes(e), [node, expected], { timeout: 30000 })
      .catch(() => {});
    return page.evaluate(read, node);
  };
  /** Press at `from`, move past the slop and on to `to`, wait until the status says `says`, and let go. */
  const drag = async (from, to, says) => {
    const [fx, fy] = await client(from);
    const [tx, ty] = await client(to);
    await page.mouse.move(fx, fy);
    await page.mouse.down();
    await page.mouse.move(fx + 8, fy + 8, { steps: 2 });
    await page.mouse.move(tx, ty, { steps: 8 });
    if (says) await page.waitForFunction((s) => document.querySelector("#status").textContent.includes(s), says, { timeout: 30000 }).catch(() => {});
    await page.waitForTimeout(150);
    const said = await status();
    await page.mouse.up();
    return said;
  };
  /** Once the status says `what`. */
  const says = (what) =>
    page.waitForFunction((w) => document.querySelector("#status").textContent.includes(w), what, { timeout: 30000 }).then(
      () => true,
      () => false,
    );
  /** A copy or a cut on the canvas, focused, as ⌘C or ⌘X fires it: what it put on the clipboard. */
  const clipboard = (type) =>
    page.evaluate((type) => {
      const data = new DataTransfer();
      const e = new ClipboardEvent(type, { clipboardData: data, bubbles: true, cancelable: true });
      document.querySelector("#overlay").focus();
      document.querySelector("#overlay").dispatchEvent(e);
      return { taken: e.defaultPrevented, clip: data.getData("application/x-scaena+json"), text: data.getData("text/plain") };
    }, type);
  /** A paste on the canvas, focused, as ⌘V fires it, of `data` by media type. */
  const pasting = (data) =>
    page.evaluate((data) => {
      const held = new DataTransfer();
      for (const [type, value] of Object.entries(data)) held.setData(type, value);
      const e = new ClipboardEvent("paste", { clipboardData: held, bubbles: true, cancelable: true });
      document.querySelector("#overlay").focus();
      document.querySelector("#overlay").dispatchEvent(e);
      return e.defaultPrevented;
    }, data);
  /** `node`'s declaration in `scn`: its first line and those after it set deeper. */
  const declared = (scn, node) => {
    const lines = scn.split("\n");
    const first = lines.findIndex((l) => l.trim().startsWith(`${node} `));
    if (first < 0) return "";
    const deeper = (l) => l.search(/\S/) > lines[first].search(/\S/);
    let last = first + 1;
    while (last < lines.length && deeper(lines[last])) last++;
    return lines.slice(first, last).join(" ");
  };
  /** One step back, to the source as the test found it, and the canvas's boxes with it. */
  const undo = async () => {
    const n = await trips();
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+z");
    await settled(n);
    await page.waitForFunction((b) => JSON.stringify(window.scaena.canvas.boxes()) === b, standing, { timeout: 30000 }).catch(() => {});
  };

  const index = await page.evaluate(() => window.scaena.opened.states.indexOf("containers"));
  const at = (await source()).indexOf("state containers") + "state ".length;
  await page.evaluate((offset) => window.scaena.cursor(offset), at);
  await page.waitForFunction((i) => window.scaena.shown() === i && window.scaena.at().index === i, index, { timeout: 30000 });
  await page.waitForFunction(() => window.scaena.canvas.boxed() === "containers" && window.scaena.canvas.boxes().length > 0, null, { timeout: 30000 });
  const original = await source();
  /** The boxes as the test found them: an undo waits for the canvas to stand so again. */
  const standing = await page.evaluate(() => JSON.stringify(window.scaena.canvas.boxes()));

  // The tally, then Shift+click on the card: both, children of the canvas.
  await click(center(await box("tally-label")));
  await page.waitForFunction(() => window.scaena.canvas.selected() === "tally-label", null, { timeout: 30000 }).catch(() => {});
  await page.keyboard.press("Escape");
  check((await chosen()).join() === "tally", `the tally: ${await chosen()}`);
  await click(center(await box("card-photo")), ["Shift"]);
  await page.waitForFunction(() => window.scaena.canvas.chosen().length === 2, null, { timeout: 30000 }).catch(() => {});
  check((await chosen()).join() === "tally,card", `Shift+click puts the card beside it, not the photo it is in: ${await chosen()}`);
  check((await page.locator("#overlay rect.selected").count()) === 2, "both are outlined");
  check((await page.locator("#overlay rect.handle").count()) === 0, "and neither has handles");
  await page.waitForFunction(() => document.querySelector("#look h2")?.textContent.startsWith("2 selected"), null, { timeout: 30000 }).catch(() => {});
  check((await page.locator("#look h2").textContent()).startsWith("2 selected"), `the inspector says what is selected: ${await page.locator("#look h2").textContent()}`);
  check((await page.locator("#look .arrange button").count()) >= 10, "and offers to align and order them");

  // A drag of the card moves both up a row: one patch, one step to undo.
  let n = await trips();
  const [cx, cy] = center(await box("card"));
  const moving = await drag([cx, cy], [cx, cy - 114], "move together");
  check(moving.includes("2 selected move together"), `the status says both move: ${moving}`);
  await settled(n);
  check(await says("2 selected moved together"), `and that they moved: ${await status()}`);
  check((await placedAs("tally", "row 4")).includes("row 4"), `the tally a row up: ${await placedAs("tally", "row 4")}`);
  check((await placedAs("card", "row 5–7")).includes("row 5–7"), `the card with it: ${await placedAs("card", "row 5–7")}`);
  check((await chosen()).join() === "tally,card", "both stay selected");
  await undo();
  check((await source()) === original, "one undo puts both back");

  // Left: the card on the tally's left edge.
  await page.waitForFunction(() => window.scaena.canvas.chosen().length === 2, null, { timeout: 30000 }).catch(() => {});
  n = await trips();
  await page.locator("#look .arrange button", { hasText: "Left" }).click();
  await settled(n);
  check(await says("tally, card: align left"), `the status says so: ${await status()}`);
  check((await placedAs("card", "col 1–4")).includes("col 1–4"), `Left aligns the card on the tally's edge: ${await placedAs("card", "col 1–4")}`);
  check((await placedAs("tally", "col 1–11")).includes("col 1–11"), "the tally stays where it is");
  await undo();
  check((await source()) === original, "one undo puts it back");

  // ⌘⇧]: the card's photo in front of everything the card holds.
  await click(center(await box("card-photo")));
  await page.waitForFunction(() => window.scaena.canvas.selected() === "card-photo", null, { timeout: 30000 }).catch(() => {});
  n = await trips();
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+Shift+BracketRight");
  await settled(n);
  check(await says("card-photo: order front"), `the status says so: ${await status()}`);
  const fronted = declared(await source(), "card-photo");
  check(/\bz:\s*2\b/.test(fronted), `its z puts it in front of the tag and its label: ${fronted.trim()}`);
  await undo();
  check((await source()) === original, "one undo puts it back");

  // A marquee from the margin across the tally and the marks; Delete takes both out.
  await page.locator("#overlay").focus();
  await page.keyboard.press("Escape");
  await page.keyboard.press("Escape");
  const [tx, ty, , th] = await box("tally");
  const [mx, , mw] = await box("marks");
  await drag([40, ty - 8], [mx + mw + 30, ty + th + 8]);
  await page.waitForFunction(() => window.scaena.canvas.chosen().length === 2, null, { timeout: 30000 }).catch(() => {});
  check(JSON.stringify((await chosen()).sort()) === JSON.stringify(["marks", "tally"]), `a marquee selects what it encloses: ${await chosen()}`);
  check(tx >= 40, "from the canvas's margin, on nothing");
  n = await trips();
  await page.locator("#overlay").focus();
  await page.keyboard.press("Delete");
  await settled(n);
  await says("2 deleted");
  const left = await page.evaluate(() => window.scaena.canvas.boxes().map((b) => b.node));
  check(!left.includes("tally") && !left.includes("marks"), `Delete takes both out of the state: ${await status()}`);
  await undo();
  check((await source()) === original, "one undo puts both back");

  // ⌘C on the tally and the card: one clip of both. ⌘V where the canvas was last pressed pastes
  // both, as they stood about each other.
  await page.evaluate(() => window.scaena.canvas.selectAll(["tally", "card"]));
  await page.locator("#overlay").focus();
  await page.evaluate(() => window.scaena.canvas.held());
  const copied = await clipboard("copy");
  const clip = JSON.parse(copied.clip || "{}");
  check(copied.taken && clip.node === "tally" && clip.more?.map((m) => m.node).join() === "card", `one clip of both: ${clip.node} and ${JSON.stringify(clip.more)}`);
  check(clip.version === 2 && Boolean(clip.nodes?.["card-photo"]), "of version 2, with what the card holds");
  check(await says("2 selected copied"), `the status says so: ${await status()}`);
  await click([960, 1000]);
  n = await trips();
  check(await pasting({ "application/x-scaena+json": copied.clip, "text/plain": copied.text }), "the canvas takes the paste");
  await settled(n);
  check(await says("tally-2, card-2 pasted in containers"), `both are pasted: ${await status()}`);
  check((await chosen()).join() === "tally-2,card-2", `and both selected: ${await chosen()}`);
  const [t2, c2, t1, c1] = await Promise.all([box("tally-2"), box("card-2"), box("tally"), box("card")]);
  check(Math.abs(c2[0] - t2[0] - (c1[0] - t1[0])) < 1 && Math.abs(c2[1] - t2[1] - (c1[1] - t1[1])) < 1, `as they stood about each other: ${t2} ${c2}`);
  await undo();
  check((await source()) === original, "one undo takes both out");

  // ⌘X: both on the clipboard, and out of the state.
  await page.evaluate(() => window.scaena.canvas.selectAll(["tally", "card"]));
  await page.locator("#overlay").focus();
  await page.evaluate(() => window.scaena.canvas.held());
  n = await trips();
  const cut = await clipboard("cut");
  await settled(n);
  await says("2 cut");
  const kept = await page.evaluate(() => window.scaena.canvas.boxes().map((b) => b.node));
  check(cut.taken && JSON.parse(cut.clip || "{}").more?.length === 1 && !kept.includes("tally") && !kept.includes("card"), `⌘X takes both out: ${await status()}`);
  await undo();
  check((await source()) === original, "one undo puts both back");

  // A stat's figure and its label: the inspector offers the role both have, with none shown, as
  // they differ; one chosen is both of theirs.
  await page.evaluate(() => window.scaena.canvas.selectAll(["stat-a-figure", "stat-a-label"]));
  await page.waitForFunction(() => document.querySelector("#look h2")?.textContent.startsWith("2 selected") && document.querySelector("#look-role"), null, { timeout: 30000 }).catch(() => {});
  check((await page.locator("#look-role").count()) === 1, "the inspector offers the role both have");
  check((await page.locator("#look-role").inputValue().catch(() => "?")) === "", "with none shown, as theirs differ");
  n = await trips();
  await page.selectOption("#look-role", "caption");
  await settled(n);
  const roles = ["stat-a-figure", "stat-a-label"].map(async (node) => /\brole:caption\b/.test(declared(await source(), node)));
  check((await Promise.all(roles)).every(Boolean), `both take it: ${declared(await source(), "stat-a-figure").trim()}`);
  await undo();
  check((await source()) === original, "one undo puts both back");
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

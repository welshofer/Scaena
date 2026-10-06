// PLAN 2.37 check: the clipboard on the editor's canvas, in headless Chromium (serve.mjs), the CPU
// painting, as in web/insert.mjs.
//
//   node web/clipboard.mjs     (after `just web`; from the repository's root)
//
// On the revenue example:
// - ⌘C on the chart puts it on the clipboard as a clip (`application/x-scaena+json`) and as its
//   text: the chart, its data source, and the CSV it reads.
// - ⌘V pastes it where the canvas was last pressed, under an id new to the deck, `rev-2`, which
//   enters in the state shown, selected, reading the same source: one patch, one undo.
// - ⌘X on the note cuts it, as Delete takes it, and ⌘V brings it back.
// - Text from anywhere else pastes as a text in the theme's body role.
// - With the keyboard: Ctrl+C and Ctrl+V copy the title through the browser's clipboard, which
//   Chromium fires from the keys only because the canvas cancels `beforecopy` and `beforepaste`,
//   and fires at a text selection an earlier click left on a button, not at the canvas.
// - Pasted into the torture deck, Dusk's subtitle comes in with the torture theme's `body` in
//   place of the `title` role it lacks, and the status says so; the chart brings its source and
//   its file, and draws.
// Exits 1 on any failure.
import { launch, serve } from "./serve.mjs";

const server = await serve();
const url = (bundle) => `${server.origin}/web/dist/editor.html?painter=cpu&bundle=${bundle}`;
const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};
const CLIP = "application/x-scaena+json";

const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1920, height: 1200 }, deviceScaleFactor: 1 });
  await context.grantPermissions(["clipboard-read", "clipboard-write"], { origin: server.origin });
  /** The editor on `bundle`, once it shows a state. */
  const open = async (bundle) => {
    const page = await context.newPage();
    page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
    page.on("console", (m) => m.type() === "error" && failures.push(`console: ${m.text()}`));
    await page.goto(url(bundle));
    await page.waitForFunction(() => window.scaena?.last() || document.querySelector("#status")?.textContent.startsWith("error"), null, {
      timeout: 120000,
    });
    return page;
  };
  const source = (page) => page.evaluate(() => window.scaena.source());
  const status = (page) => page.evaluate(() => document.querySelector("#status").textContent);
  const selected = (page) => page.evaluate(() => window.scaena.canvas.selected());
  const box = (page, node) => page.evaluate((n) => window.scaena.canvas.boxes().find((b) => b.node === n)?.rect, node);
  /** Once the source holds `text` (or, with `absent`, does not), and the canvas has caught up. */
  const reads = async (page, text, absent = false) => {
    await page
      .waitForFunction(([t, a]) => window.scaena.source().includes(t) !== a && !window.scaena.canvas.busy(), [text, absent], {
        timeout: 30000,
        polling: 50,
      })
      .catch(() => {});
    await page.waitForTimeout(300);
    return (await source(page)).includes(text) !== absent;
  };
  const back = async (page, to) => {
    await page.waitForFunction((s) => window.scaena.source() === s, to, { timeout: 30000, polling: 50 }).catch(() => {});
    await page.waitForTimeout(300);
    return (await source(page)) === to;
  };
  /** Once the status says `text`. */
  const says = async (page, text) => {
    await page.waitForFunction((t) => document.querySelector("#status").textContent.includes(t), text, { timeout: 30000 }).catch(() => {});
    return (await status(page)).includes(text);
  };
  /** Show the state declared as `state` in the source. */
  const showState = async (page, state) => {
    const index = await page.evaluate((s) => window.scaena.opened.states.indexOf(s), state);
    const at = (await source(page)).indexOf(`state ${state}`) + "state ".length;
    await page.evaluate((offset) => window.scaena.cursor(offset), at);
    await page.waitForFunction((i) => window.scaena.shown() === i && window.scaena.at().index === i, index, { timeout: 30000 });
    await page.waitForFunction(() => window.scaena.canvas.boxes().length > 0, null, { timeout: 30000 });
  };
  /** Select `node` on the canvas, the canvas focused, as a click does, and wait for its clip. */
  const select = async (page, node) => {
    await page.evaluate((n) => window.scaena.canvas.select(n), node);
    await page.locator("#overlay").focus();
    await page.evaluate(() => window.scaena.canvas.held());
  };
  /** Press the canvas at `at`, canvas units. */
  const press = async (page, [x, y]) => {
    const [cx, cy] = await page.evaluate(
      ([x, y]) => {
        const r = document.querySelector("#overlay").getBoundingClientRect();
        const [w, h] = window.scaena.canvas.size();
        return [r.left + (x / w) * r.width, r.top + (y / h) * r.height];
      },
      [x, y],
    );
    await page.mouse.click(cx, cy);
    await page.waitForFunction(() => !window.scaena.canvas.busy(), null, { timeout: 30000 });
  };
  /** A copy or a cut on the canvas, focused, as ⌘C or ⌘X fires it: what it put on the clipboard. */
  const copy = (page, type = "copy") =>
    page.evaluate((type) => {
      const data = new DataTransfer();
      const e = new ClipboardEvent(type, { clipboardData: data, bubbles: true, cancelable: true });
      document.querySelector("#overlay").focus();
      document.querySelector("#overlay").dispatchEvent(e);
      return { taken: e.defaultPrevented, clip: data.getData("application/x-scaena+json"), text: data.getData("text/plain") };
    }, type);
  /** A paste on the canvas, focused, as ⌘V fires it, of `data` by media type. */
  const paste = (page, data) =>
    page.evaluate((data) => {
      const held = new DataTransfer();
      for (const [type, value] of Object.entries(data)) held.setData(type, value);
      const e = new ClipboardEvent("paste", { clipboardData: held, bubbles: true, cancelable: true });
      document.querySelector("#overlay").focus();
      document.querySelector("#overlay").dispatchEvent(e);
      return e.defaultPrevented;
    }, data);

  const page = await open("/docs/examples/revenue.deck.json");
  await showState(page, "revenue");
  const original = await source(page);

  // ⌘C on the chart: a clip, as JSON and as text, with what it reads.
  await select(page, "rev");
  const copied = await copy(page);
  check(copied.taken, "the canvas takes the copy");
  check(copied.clip.length > 0 && copied.clip === copied.text, "the clip goes on the clipboard as JSON and as text");
  const clip = JSON.parse(copied.clip || "{}");
  check(clip.kind === "scaena/clip" && clip.node === "rev", `a clip of the chart: ${clip.kind} ${clip.node}`);
  check(clip.nodes?.rev?.type === "chart" && clip.nodes.rev.data === "@q3", "the chart as the state shows it");
  check(Boolean(clip.data?.q3) && Boolean(clip.files?.["data/q3-revenue.csv"]), "with its data source and the CSV it reads");
  check(await says(page, "rev copied"), `the status says so: ${await status(page)}`);

  // ⌘V where the canvas was last pressed: `rev-2`, entering in `revenue`, selected.
  const at = [600, 820];
  await press(page, at);
  check(await paste(page, { [CLIP]: copied.clip, "text/plain": copied.text }), "the canvas takes the paste");
  check(await reads(page, "rev-2 chart"), "the chart is pasted as `rev-2`");
  check(await says(page, "rev-2 pasted in revenue"), `the status says where: ${await status(page)}`);
  check((await selected(page)) === "rev-2", "and the canvas selects it");
  const scn = await source(page);
  check(scn.indexOf("rev-2 chart") > scn.indexOf("state revenue") && scn.indexOf("rev-2 chart") < scn.indexOf("state mix"), "it enters in `revenue`");
  check(scn.split("@q3").length === original.split("@q3").length + 1, "reading the same source, declared once");
  const cell = await box(page, "rev-2");
  check(Boolean(cell) && cell[0] <= at[0] && at[0] <= cell[0] + cell[2] && cell[1] <= at[1] && at[1] <= cell[1] + cell[3], `about the point pressed: ${cell}`);
  await page.keyboard.press("Control+z");
  check(await back(page, original), "one undo takes the paste back");

  // ⌘X on the note: it goes as Delete takes it; ⌘V brings it back where the canvas was pressed.
  await select(page, "note");
  const cut = await copy(page, "cut");
  check(cut.taken && JSON.parse(cut.clip || "{}").node === "note", "a cut puts the note on the clipboard");
  check(await reads(page, '"Revenue in $M', true), "and takes it out");
  check(await says(page, "note cut from"), `the status says so: ${await status(page)}`);
  const afterCut = await source(page);
  await press(page, [960, 1000]);
  check(await paste(page, { [CLIP]: cut.clip }), "the canvas takes the paste");
  check(await reads(page, '"Revenue in $M'), "the note comes back");
  check(/^\s*note(-2)? text/m.test(await source(page)), "under its id, or the next free one");
  await page.keyboard.press("Control+z");
  check(await back(page, afterCut), "one undo takes the paste back");
  await page.keyboard.press("Control+z");
  check(await back(page, original), "and one more, the cut");

  // Text from anywhere else: a text in the body role.
  await press(page, [960, 540]);
  check(await paste(page, { "text/plain": "Margins held.\n" }), "the canvas takes text");
  check(await reads(page, 'body text role:body "Margins held."'), "it is pasted as a text in the body role");
  check((await selected(page)) === "body", "selected");
  await page.keyboard.press("Control+z");
  check(await back(page, original), "one undo takes it back");

  // With the keyboard, through the browser's own clipboard (the page in front, as a person's is).
  await page.bringToFront();
  await select(page, "title");
  await page.keyboard.press("Control+c");
  await page.waitForTimeout(300);
  const held = await page.evaluate(() => navigator.clipboard.readText().catch((e) => `error: ${e.message}`));
  check(JSON.parse(held.startsWith("{") ? held : "{}").node === "title", `Ctrl+C puts the title on the clipboard: ${held.slice(0, 80)}`);
  await press(page, [1500, 980]);
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+v");
  check(await reads(page, "title-2 text"), "Ctrl+V pastes it as `title-2`");
  await page.keyboard.press("Control+z");
  check(await back(page, original), "one undo takes it back");

  // From one deck into another: the torture deck, whose theme has no `title` role.
  await showState(page, "intro");
  await select(page, "subtitle");
  const subtitle = await copy(page);
  await showState(page, "revenue");
  await select(page, "rev");
  const chart = await copy(page);
  const torture = await open("/tests/fixtures/torture.scaena/deck.json");
  await showState(torture, "shapes");
  await press(torture, [960, 980]);
  check(await paste(torture, { [CLIP]: subtitle.clip }), "the torture deck takes the paste");
  check(await reads(torture, "subtitle text role:body"), "Dusk's subtitle comes in with `body` for the `title` role its theme lacks");
  check(await says(torture, "role `title` is not in the theme"), `the status says what it lacked: ${await status(torture)}`);
  check((await status(torture)).includes("`body` in its place"), "and what took its place");
  await press(torture, [1500, 300]);
  check(await paste(torture, { [CLIP]: chart.clip }), "and the chart");
  check(await reads(torture, "rev chart"), "the chart comes in");
  check((await source(torture)).includes("data/q3-revenue.csv"), "declaring its source, on the file it brought");
  await torture.waitForFunction(() => window.scaena.canvas.boxes().some((b) => b.node === "rev" && b.draws), null, { timeout: 30000 }).catch(() => {});
  check(Boolean(await box(torture, "rev")), "and it draws, its rows read");
  await torture.close();

} finally {
  await browser.close();
  server.close();
}
if (failures.length) {
  console.error(`\n${failures.length} failure(s):\n${failures.join("\n")}`);
  process.exit(1);
}
console.log("\nthe clipboard copies, cuts, and pastes");

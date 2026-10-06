// PLAN 2.59 check: the bundle's files in the editor's Files tab, in headless Chromium (serve.mjs),
// the CPU painting, on the torture deck, with an image added that nothing names (its path left in
// a comment of the source).
//
//   node web/files.mjs     (after `just web`; from the repository's root)
//
// - The tab lists the bundle's images, fonts, and data, each with its size and what names it: the
//   test card by the image nodes that show it, a font by the deck's fonts and the theme's family,
//   a data file by its source; and the nodes drawn from each, a button that shows the first state
//   that shows it so, the node selected.
// - The image nothing names says so; Remove takes it out, one change: the panel's Undo puts it
//   back, and Redo takes it out again. One something names cannot be taken out.
// - An image dragged from the list carries its path. Dropped on an image node, the node shows it
//   (`choose` of `src`); dropped on empty canvas, it is inserted there; Insert does the same from
//   the keyboard. Each is one patch, one step to undo. Dropped on the source, its path goes
//   there, quoted.
// Exits 1 on any failure.
import { readFileSync } from "node:fs";
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
  const says = (s) => page.waitForFunction((t) => document.querySelector("#status").textContent.includes(t), s, { timeout: 30000 }).catch(() => {});
  const listed = () => page.evaluate(() => window.scaena.files.listed());
  const file = async (path) => (await listed()).find((f) => f.path === path);
  /** Show the state declared as `state` in the source, and what stands where in it. */
  const into = async (state) => {
    await page.evaluate((s) => window.scaena.cursor(window.scaena.source().indexOf(`state ${s}`) + "state ".length), state);
    await page.waitForFunction((s) => window.scaena.canvas.boxed() === s && window.scaena.canvas.boxes().length > 0, state, { timeout: 30000 });
  };
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
  /** `path` dragged from the panel and dropped at canvas point `at`: what the panel's row puts on
   * the drag, handed to the canvas's drop. */
  const dropAt = async (path, at) => {
    const [x, y] = await client(at);
    return page.evaluate(
      ([path, x, y]) => {
        const row = document.querySelector(`#files [data-path="${path}"]`);
        const dt = new DataTransfer();
        row.dispatchEvent(new DragEvent("dragstart", { dataTransfer: dt, bubbles: true, cancelable: true }));
        const carried = { path: dt.getData("application/x-scaena-path"), text: dt.getData("text/plain") };
        const overlay = document.querySelector("#overlay");
        for (const type of ["dragover", "drop"]) overlay.dispatchEvent(new DragEvent(type, { dataTransfer: dt, clientX: x, clientY: y, bubbles: true, cancelable: true }));
        return carried;
      },
      [path, x, y],
    );
  };
  const undo = async (to) => {
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+z");
    await page.waitForFunction((s) => window.scaena.source() === s, to, { timeout: 30000, polling: 50 }).catch(() => {});
    return (await source()) === to;
  };

  // An image nothing names: dropped on the source, its path left in a comment there.
  const card = readFileSync(new URL("../tests/fixtures/torture.scaena/assets/test-card.png", import.meta.url));
  await page.evaluate(() => window.scaena.type(`${window.scaena.source()}\n# spare: `));
  const [stray] = await page.evaluate(
    async (bytes) => window.scaena.drop("spare.png", new Uint8Array(bytes).buffer, window.scaena.source().length),
    [...card],
  );
  await page.waitForFunction(() => window.scaena.last()?.valid, null, { timeout: 60000 });
  await into("images");
  const original = await source();

  // The tab lists what the bundle holds, and what uses each.
  await page.locator("#tab-files").click();
  await page.waitForFunction(() => window.scaena.files.listed().length > 0, null, { timeout: 30000 });
  const summary = await page.evaluate(() => document.querySelector("#files [data-summary]").textContent);
  check(/\d+ images? · \d+ fonts? · \d+ data files? · 1 file nothing names/.test(summary), `the tab says what the bundle holds: ${summary}`);
  const test = await file("assets/test-card.png");
  check(test?.named.some((n) => n.by === "node" && n.node === "image-cover"), `the test card is named by the images that show it: ${JSON.stringify(test?.named.slice(0, 3))}`);
  check(test?.used.some((u) => u.node === "image-cover" && u.states.includes("images")), "and drawn by image-cover in images");
  const font = (await listed()).find((f) => f.type === "font" && f.named.some((n) => n.by === "theme"));
  check(font?.named.some((n) => n.by === "font"), `a font by the deck's fonts and the theme's family: ${font?.path}`);
  const bars = await file("data/bars.csv");
  check(bars?.named[0]?.by === "source" && bars.used.length > 0, `a data file by its source, and the charts that read it: ${JSON.stringify(bars?.named)}`);
  const spare = await file(stray);
  check(spare?.named.length === 0, `the image nothing names says so: ${stray}`);
  const row = await page.evaluate((p) => document.querySelector(`#files [data-path="${p}"]`)?.textContent, stray);
  check(row?.includes("Nothing names it") && row.includes("Remove"), `and offers to take it out: ${row?.trim()}`);

  // A node a file draws shows its state, the node selected.
  await into("axes");
  await page.waitForFunction(() => window.scaena.files.listed().length > 0, null, { timeout: 30000 });
  await page.locator('#files [data-path="assets/test-card.png"] [data-node="image-cover"]').click();
  await page.waitForFunction(() => window.scaena.canvas.boxed() === "images" && window.scaena.canvas.selected() === "image-cover", null, { timeout: 30000 }).catch(() => {});
  check(await page.evaluate(() => window.scaena.canvas.selected() === "image-cover" && window.scaena.canvas.boxed() === "images"), "a node's button shows its state, the node selected");

  // Remove takes it out; the panel's Undo puts it back, and Redo takes it out again.
  await page.locator(`#files [data-path="${stray}"] [data-remove]`).click();
  await says("taken out of the bundle");
  check(!(await file(stray)), `Remove takes it out: ${await status()}`);
  check((await source()) === original, "and the source is as it was");
  await page.locator("#files [data-undo]").click();
  await says("undone");
  await page.waitForFunction((p) => window.scaena.files.listed().some((f) => f.path === p), stray, { timeout: 30000 }).catch(() => {});
  check(Boolean(await file(stray)), `Undo puts it back: ${await status()}`);
  await page.locator("#files [data-redo]").click();
  await says("made again");
  await page.waitForFunction((p) => !window.scaena.files.listed().some((f) => f.path === p), stray, { timeout: 30000 }).catch(() => {});
  check(!(await file(stray)), "and Redo takes it out again");
  await page.locator("#files [data-undo]").click();
  await page.waitForFunction((p) => window.scaena.files.listed().some((f) => f.path === p), stray, { timeout: 30000 }).catch(() => {});
  // One something names cannot be.
  const refused = await page.evaluate(() => window.scaena.files.remove("data/bars.csv").then(() => document.querySelector("#status").textContent));
  check(refused.startsWith("not taken out") && refused.includes("data source"), `one something names stays: ${refused}`);

  // Dragged onto an image node, the node shows it: one `choose` of `src`.
  const boxes = await page.evaluate(() => window.scaena.canvas.boxes());
  const cover = boxes.find((b) => b.node === "image-cover").rect;
  const carried = await dropAt(stray, [cover[0] + cover[2] / 2, cover[1] + cover[3] / 2]);
  check(carried.path === stray && carried.text === JSON.stringify(stray), `a row carries its path, and its path quoted for the source: ${JSON.stringify(carried)}`);
  await says(`image-cover shows ${stray}`);
  // Each edit on the canvas writes the source canonically, without the comment.
  const line = (await source()).split("\n").find((l) => l.trim().startsWith("image-cover "));
  check(line?.includes(JSON.stringify(stray)), `dropped on image-cover, it shows the image: ${line?.trim()} · ${await status()}`);
  await page.waitForFunction((p) => window.scaena.files.listed().find((f) => f.path === p)?.named.length > 0, stray, { timeout: 30000 }).catch(() => {});
  check((await file(stray))?.named.some((n) => n.by === "node" && n.node === "image-cover"), "and the panel says image-cover names it now");
  check(await undo(original), "one undo takes it back");

  // Dropped on empty canvas, it is inserted there.
  const empty = [1700, 1000];
  await dropAt(stray, empty);
  await says("inserted as");
  const inserted = await status();
  check(inserted.startsWith(`${stray} inserted as image`), `dropped on empty canvas, it is inserted: ${inserted}`);
  const id = inserted.match(/inserted as (\S+),/)?.[1];
  const at = await page.evaluate((n) => window.scaena.canvas.boxes().find((b) => b.node === n)?.rect, id);
  check(at && empty[0] >= at[0] - 1 && empty[1] >= at[1] - 1, `about where it was dropped: ${JSON.stringify(at)}`);
  check(await undo(original), "one undo takes it back");

  // Insert does the same from the keyboard.
  await page.locator(`#files [data-path="${stray}"] [data-insert]`).focus();
  await page.keyboard.press("Enter");
  await page.waitForFunction((s) => window.scaena.source() !== s, original, { timeout: 30000 }).catch(() => {});
  await says("inserted as");
  check((await status()).startsWith(`${stray} inserted as image`), `Insert inserts it: ${await status()}`);
  check(await undo(original), "and one undo takes that back");

  // Dropped on the source, its path goes where it is dropped, quoted.
  await page.evaluate(() => {
    window.scaena.cursor(window.scaena.source().length);
    const scroller = document.querySelector(".cm-scroller");
    scroller.scrollTop = scroller.scrollHeight;
  });
  await page.waitForFunction(() => {
    const lines = document.querySelectorAll(".cm-content .cm-line");
    return lines[lines.length - 1]?.textContent === window.scaena.source().split("\n").pop();
  }, null, { timeout: 30000 });
  await page.evaluate((path) => {
    const row = document.querySelector(`#files [data-path="${path}"]`);
    const dt = new DataTransfer();
    row.dispatchEvent(new DragEvent("dragstart", { dataTransfer: dt, bubbles: true, cancelable: true }));
    const lines = document.querySelectorAll(".cm-content .cm-line");
    const r = lines[lines.length - 1].getBoundingClientRect();
    const at = { clientX: r.right - 2, clientY: r.top + r.height / 2 };
    const content = document.querySelector(".cm-content");
    for (const type of ["dragover", "drop"]) content.dispatchEvent(new DragEvent(type, { dataTransfer: dt, ...at, bubbles: true, cancelable: true }));
  }, stray);
  await page.waitForFunction((s) => window.scaena.source() !== s, original, { timeout: 30000 }).catch(() => {});
  const times = (s) => s.split(JSON.stringify(stray)).length - 1;
  check(times(await source()) === times(original) + 1 && (await source()).trimEnd().endsWith(JSON.stringify(stray).repeat(2)), "dropped on the source, its path goes there, quoted");
  await page.locator(".cm-content").focus();
  await page.keyboard.press("Control+z");
  await page.waitForFunction((s) => window.scaena.source() === s, original, { timeout: 30000 }).catch(() => {});
  check((await source()) === original, "and the source's undo takes it back");
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
console.log("\nall files checks pass");

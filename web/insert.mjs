// PLAN 2.34 check: nodes inserted, copied, and deleted on the editor's canvas, in headless Chromium
// (serve.mjs), the CPU painting, as in web/canvas.mjs.
//
//   node web/insert.mjs     (after `just web`; from the repository's root)
//
// On the revenue example:
// - The Insert menu offers what Dusk and the bundle name: a text in each of the theme's roles, four
//   shapes, and the theme's two shader presets; once a PNG is dropped into the bundle, the image.
// - A kicker inserted where the canvas was last pressed, in the template's `kicker` slot, which
//   nothing fills in `revenue`, fills it, and enters in `revenue`, selected: one patch, one undo.
// - A headline inserted over the chart, whose slot is filled, takes the grid's cells in the room
//   nearest the point pressed, clear of the chart (PLAN 2.79).
// - Delete takes a node just inserted out of the deck again, and the source is as it was. On the
//   title in `mix`, it takes the title out of `mix` alone (`-title`): `close` shows it again by its
//   own delta.
// - ⌘D copies the title beside it as `title-2`, selected, and Shift+Delete takes the copy out of
//   the deck.
// - A shader preset fills the canvas under the rest, and the image dropped is inserted.
// Exits 1 on any failure.
import { readFile } from "node:fs/promises";
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
  await page.waitForFunction(() => window.scaena?.last() || document.querySelector("#status")?.textContent.startsWith("error"), null, {
    timeout: 120000,
  });

  const source = () => page.evaluate(() => window.scaena.source());
  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  const selected = () => page.evaluate(() => window.scaena.canvas.selected());
  const box = (node) => page.evaluate((n) => window.scaena.canvas.boxes().find((b) => b.node === n)?.rect, node);
  /** Once the source holds `text` (or, with `absent`, does not), and the canvas has caught up. */
  const reads = async (text, absent = false) => {
    await page
      .waitForFunction(([t, a]) => window.scaena.source().includes(t) !== a && !window.scaena.canvas.busy(), [text, absent], {
        timeout: 30000,
        polling: 50,
      })
      .catch(() => {});
    await page.waitForTimeout(300);
    return (await source()).includes(text) !== absent;
  };
  const back = async (to) => {
    await page.waitForFunction((s) => window.scaena.source() === s, to, { timeout: 30000, polling: 50 }).catch(() => {});
    await page.waitForTimeout(300);
    return (await source()) === to;
  };
  /** Once the status says `text`. */
  const says = async (text) => {
    await page.waitForFunction((t) => document.querySelector("#status").textContent.includes(t), text, { timeout: 30000 }).catch(() => {});
    return (await status()).includes(text);
  };
  /** Show the state declared as `state` in the source, slot `index`. */
  const showState = async (state, index) => {
    const at = (await source()).indexOf(`state ${state}`) + "state ".length;
    await page.evaluate((offset) => window.scaena.cursor(offset), at);
    await page.waitForFunction((i) => window.scaena.shown() === i && window.scaena.at().index === i, index, { timeout: 30000 });
    // The canvas's boxes are the shown state's, not the one shown before it.
    await page.waitForFunction((s) => window.scaena.canvas.boxed() === s && window.scaena.canvas.boxes().length > 0, state, { timeout: 30000 });
  };
  /** Select `node` on the canvas, the canvas focused, as a click does. */
  const select = async (node) => {
    await page.evaluate((n) => window.scaena.canvas.select(n), node);
    await page.locator("#overlay").focus();
  };
  /** What the Insert menu offers: each group's label, and its items. */
  const menu = () =>
    page.evaluate(() =>
      Object.fromEntries([...document.querySelectorAll("#insert optgroup")].map((g) => [g.label, [...g.children].map((o) => o.textContent)])),
    );
  /** Press the canvas at `at`, canvas units. */
  const press = async ([x, y]) => {
    const [cx, cy] = await page.evaluate(
      ([x, y]) => {
        const r = document.querySelector("#overlay").getBoundingClientRect();
        const [w, h] = window.scaena.canvas.size();
        return [r.left + (x / w) * r.width, r.top + (y / h) * r.height];
      },
      [x, y],
    );
    await page.mouse.click(cx, cy);
  };

  await showState("revenue", 1);
  const original = await source();

  // What the menu offers: Dusk's roles, the shapes, the shader presets, and no image yet.
  await page.waitForFunction(() => document.querySelectorAll("#insert optgroup").length > 0, null, { timeout: 30000 });
  const offered = await menu();
  check(["display", "headline", "title", "caption"].every((r) => offered.Text?.includes(r)), `a text in each of Dusk's roles: ${offered.Text}`);
  check(JSON.stringify(offered.Shape) === JSON.stringify(["rect", "ellipse", "line", "arrow"]), `four shapes: ${offered.Shape}`);
  check(offered.Shader?.length === 2, `the theme's shader presets: ${offered.Shader}`);
  check(!offered.Image, "no image: the bundle holds no PNG");

  // A kicker in the `kicker` slot, which nothing fills in `revenue`: it fills the slot.
  const slots = (await page.evaluate(() => window.scaena.canvas.targets("title"))).slots ?? {};
  const kicker = slots.kicker;
  check(Boolean(kicker), `the figure layout has a kicker slot: ${Object.keys(slots)}`);
  await press([kicker[0] + kicker[2] / 2, kicker[1] + kicker[3] / 2]);
  await page.selectOption("#insert", { label: "kicker" });
  check(await reads('kicker text role:kicker "Kicker" at:in(kicker)'), "a kicker inserted in the empty kicker slot fills it");
  check(await says("kicker inserted as kicker, in revenue"), `the status says what was inserted: ${await status()}`);
  check((await selected()) === "kicker", "and the canvas selects it");
  check(JSON.stringify(await box("kicker")) === JSON.stringify(kicker), `it stands in the slot: ${await box("kicker")}`);
  const scn = await source();
  check(scn.indexOf("kicker text") > scn.indexOf("state revenue") && scn.indexOf("kicker text") < scn.indexOf("state mix"), "it enters in `revenue`");
  await page.keyboard.press("Control+z");
  check(await back(original), "one undo takes the insert back");

  // A headline pressed over the chart, whose slot is filled: the chart is there, so it takes the
  // grid's cells in the room nearest the point, clear of what lint judges it against (PLAN 2.79).
  const at = [1400, 300];
  await press(at);
  await page.selectOption("#insert", { label: "headline" });
  check(await reads("headline text role:headline"), "the headline is inserted into the source");
  check((await source()).includes('headline text role:headline "Headline" at:col('), "on the grid's cells");
  const cell = await box("headline");
  const overlaps = await page.evaluate(() => window.scaena.last().findings.filter((f) => f.code === "E101" && f.message.includes("`headline`")).map((f) => f.message));
  const on = Boolean(cell) && cell[0] <= at[0] && at[0] <= cell[0] + cell[2] && cell[1] <= at[1] && at[1] <= cell[1] + cell[3];
  check(Boolean(cell) && !on && overlaps.length === 0, `off the chart, in the room nearest the point: ${cell} · ${overlaps.join("; ") || "no overlap lint finds"}`);

  // Inserted, then deleted: nothing is left behind.
  await page.waitForFunction(() => window.scaena.canvas.selected() === "headline", null, { timeout: 30000 }).catch(() => {});
  await page.locator("#overlay").focus();
  await page.keyboard.press("Delete");
  check(await back(original), "Delete takes a node just inserted out of the deck: the source is as it was");
  check(await says("headline deleted from the deck"), `and says so: ${await status()}`);

  // The title deleted in `mix`: it leaves `mix`, and `close` shows it again by its own delta.
  await showState("mix", 2);
  await select("title");
  await page.keyboard.press("Delete");
  check(await reads("state mix slide:revenue transition:slow hold:6s\n  -title"), "Delete takes the title out of `mix`");
  check(!(await source()).includes("…and the mix shifted"), "with its delta there");
  check((await source()).includes('title "Thank you"'), "`close` shows it again by its own");
  check(await says("title deleted from mix on"), `the status says where from: ${await status()}`);
  check((await selected()) === undefined, "nothing is selected after");
  await page.keyboard.press("Control+z");
  check(await back(original), "one undo puts it back");

  // ⌘D copies the title beside it; Shift+Delete takes the copy out of the deck.
  await showState("revenue", 1);
  await select("title");
  await page.keyboard.press("Control+d");
  check(await reads("title-2 text"), "Ctrl+D copies the title as `title-2`");
  check(await says("title copied as title-2"), `the status says so: ${await status()}`);
  check((await selected()) === "title-2", "and the canvas selects the copy");
  const [title, copy] = [await box("title"), await box("title-2")];
  check(Boolean(title && copy) && (copy[0] >= title[0] + title[2] || copy[1] >= title[1] + title[3]), `beside the title, clear of it: ${title} / ${copy}`);
  await page.keyboard.press("Shift+Delete");
  check(await back(original), "Shift+Delete takes the copy out of the deck");

  // A shader preset fills the canvas, under the rest.
  const preset = offered.Shader[0];
  await page.selectOption("#insert", { label: preset });
  check(await reads(`${preset} shader:`), `the ${preset} preset is inserted`);
  const filled = await box(preset);
  const [w, h] = await page.evaluate(() => window.scaena.canvas.size());
  check(JSON.stringify(filled) === JSON.stringify([0, 0, w, h]), `it fills the canvas: ${filled}`);
  check((await source()).includes("z:-1"), "under what is there");
  await page.keyboard.press("Control+z");
  check(await back(original), "one undo takes it back");

  // An image dropped into the bundle is offered, and inserted.
  const png = await readFile("tests/fixtures/torture.scaena/assets/test-card.png");
  const [path] = await page.evaluate((bytes) => window.scaena.drop("card.png", new Uint8Array(bytes).buffer, 0), [...png]);
  // Named by its SHA-256, and offered by its first eight digits.
  const hash = path.slice("assets/".length, "assets/".length + 8);
  // The drop wrote the image's path into the source: the source as it was.
  await page.evaluate((s) => window.scaena.type(s), original);
  await page.waitForFunction(() => document.querySelector("#insert optgroup[label=Image]"), null, { timeout: 30000 }).catch(() => {});
  const images = (await menu()).Image ?? [];
  check(images.length === 1 && images[0] === `${hash}…`, `the image dropped is offered by its hash: ${images}`);
  if (images.length === 1) {
    await press([960, 540]);
    await page.selectOption("#insert", { label: images[0] });
    check(await reads(JSON.stringify(path)), "and inserted, its path its `src`");
    const id = await selected();
    check(id === `image-${hash}` && Boolean(await box(id)), `it stands on the canvas, selected: ${id}`);
    await page.keyboard.press("Delete");
    check(await back(original), "Delete takes it out again");
  }
  check(await page.evaluate(() => window.scaena.last().valid), "the source still compiles and validates");
} finally {
  await browser.close();
  await server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "nodes are inserted, copied, and deleted, each a patch");
process.exit(failures.length ? 1 : 0);

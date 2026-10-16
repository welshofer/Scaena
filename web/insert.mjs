// PLAN 2.34 check: nodes inserted, copied, and deleted on the editor's canvas, in headless Chromium
// (serve.mjs), the CPU painting, as in web/canvas.mjs.
//
//   node web/insert.mjs     (after `just web`; from the repository's root)
//
// On the revenue example:
// - The Insert menu offers what Dusk and the bundle name: a text in each of the theme's roles, the
//   shapes and the shape library's polygons (PLAN 2.106), and the theme's two shader presets; once
//   a PNG is dropped into the bundle, the image. Under Draw, the draw tools with their keys (PLAN
//   2.123): each chosen does what its key does, Rectangle arming the canvas as R does and Pen
//   taking up the pen as P does.
// - A kicker inserted where the canvas was last pressed, in the template's `kicker` slot, which
//   nothing fills in `revenue`, fills it, and enters in `revenue`, selected: one patch, one undo.
//   It is typed in at once, its words selected, so what is typed takes their place (PLAN 3.21),
//   no key of it taken as the canvas's own; one more undo takes the typing back.
// - A headline inserted over the chart, whose slot is filled, takes the grid's cells in the room
//   nearest the point pressed, clear of the chart (PLAN 2.79).
// - With the chart deleted, a chart of the source pressed into the figure's place fills it.
// - Delete takes a node just inserted out of the deck again, and the source is as it was. On the
//   title in `mix`, it takes the title out of `mix` alone (`-title`): `close` shows it again by its
//   own delta.
// - ⌘D copies the title beside it as `title-2`, selected, and Shift+Delete takes the copy out of
//   the deck.
// - A star from the shape library is a polygon of ten points in the accent, named Star on the menu
//   and in the layers.
// - A shader preset fills the canvas under the rest, and the image dropped is inserted in its own
//   proportions, which a drag of its corner keeps, let go beside another's edge too.
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
  await page.waitForFunction(() => window.scaena?.last() || document.querySelector("#status")?.textContent.startsWith("Error"), null, {
    timeout: 120000,
  });

  const source = () => page.evaluate(() => window.scaena.source());
  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  const selected = () => page.evaluate(() => window.scaena.canvas.selected());
  /** Once the canvas selects `id`, which it does once the edit that made it is shown: whether it
   * does within the wait. */
  const selects = (id) =>
    page
      .waitForFunction((n) => window.scaena.canvas.selected() === n, id, { timeout: 10000 })
      .then(() => true, () => false);
  /** `node`'s box on the canvas, once the canvas has one for it. */
  const box = async (node) => {
    await page.waitForFunction((n) => window.scaena.canvas.boxes().some((b) => b.node === n), node, { timeout: 10000 }).catch(() => {});
    return page.evaluate((n) => window.scaena.canvas.boxes().find((b) => b.node === n)?.rect, node);
  };
  /** `node`'s box once the canvas has drawn it, which it does once the edit that made it is shown. */
  const drawn = (node) =>
    page
      .waitForFunction((n) => window.scaena.canvas.boxes()?.find((b) => b.node === n)?.rect, node, { timeout: 10000 })
      .then((h) => h.jsonValue(), () => undefined);
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
    // Once the canvas has caught up with the last edit, whose undo may show the state it changed.
    await page.waitForFunction(() => !window.scaena.canvas.busy(), null, { timeout: 30000 }).catch(() => {});
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
  /** A point in canvas units, on the page. */
  const clientOf = ([x, y]) =>
    page.evaluate(
      ([x, y]) => {
        const r = document.querySelector("#overlay").getBoundingClientRect();
        const [w, h] = window.scaena.canvas.size();
        return [r.left + (x / w) * r.width, r.top + (y / h) * r.height];
      },
      [x, y],
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
  check(["Display", "Headline", "Title", "Caption"].every((r) => offered.Text?.includes(r)), `a text in each of Dusk's roles, by its name: ${offered.Text}`);
  check(!offered.Text?.includes("Axis"), "but for Axis, which Dusk sets only a chart's text in");
  // The rectangle and the oval, the shape library's polygons (PLAN 2.106), then the line and the arrow.
  const shapes = [
    "Rectangle", "Oval", "Triangle", "Right Triangle", "Diamond", "Parallelogram", "Trapezoid", "Pentagon", "Hexagon",
    "Octagon", "Star", "Chevron", "Right Arrow", "Left Arrow", "Up Arrow", "Down Arrow", "Plus", "Callout", "Line", "Arrow",
  ];
  check(JSON.stringify(offered.Shape) === JSON.stringify(shapes), `the shapes, by their names: ${offered.Shape}`);
  check(offered["Background Effect"]?.length === 2, `the theme's shader presets: ${offered["Background Effect"]}`);
  check(!offered.Image, "no image: the bundle holds no PNG");

  // The draw tools, each with its key, each its key's gesture (PLAN 2.123).
  const tools = ["Text (T)", "Rectangle (R)", "Oval (O)", "Line (L)", "Arrow (A)", "Pen (P)"];
  check(JSON.stringify(offered.Draw) === JSON.stringify(tools), `Draw lists the draw tools with their keys: ${offered.Draw}`);
  await page.selectOption("#insert", { label: "Rectangle (R)" });
  await page.waitForFunction(() => window.scaena.canvas.armed()?.key === "r", null, { timeout: 10000 }).catch(() => {});
  check((await page.evaluate(() => window.scaena.canvas.armed()?.key)) === "r" && (await says("Drawing a rectangle")), `Rectangle arms the canvas as R does: ${await status()}`);
  check(await page.evaluate(() => document.activeElement?.id === "overlay"), "and the slide has the keys, for the drag and for Escape");
  await page.selectOption("#insert", { label: "Oval (O)" });
  await page.waitForFunction(() => window.scaena.canvas.armed()?.key === "o", null, { timeout: 10000 }).catch(() => {});
  check(await says("Drawing an oval"), `Oval arms it for an oval instead: ${await status()}`);
  await page.keyboard.press("Escape");
  check((await page.evaluate(() => window.scaena.canvas.armed())) === undefined, "Escape stops, as after O");
  await page.selectOption("#insert", { label: "Pen (P)" });
  await page.waitForFunction(() => window.scaena.canvas.pen() !== undefined, null, { timeout: 10000 }).catch(() => {});
  check((await page.evaluate(() => window.scaena.canvas.pen())) !== undefined && (await says("Drawing with the pen")), `Pen takes up the pen as P does: ${await status()}`);
  await page.selectOption("#insert", { label: "Pen (P)" });
  check((await page.evaluate(() => window.scaena.canvas.pen())) !== undefined, "chosen again, it stays taken up");
  await page.keyboard.press("Escape");
  check((await page.evaluate(() => window.scaena.canvas.pen())) === undefined && (await source()) === original, "Escape puts it down, and nothing is drawn");

  // A kicker in the `kicker` slot, which nothing fills in `revenue`: it fills the slot.
  const slots = (await page.evaluate(() => window.scaena.canvas.targets("title"))).slots ?? {};
  const kicker = slots.kicker;
  check(Boolean(kicker), `the figure layout has a kicker slot: ${Object.keys(slots)}`);
  await press([kicker[0] + kicker[2] / 2, kicker[1] + kicker[3] / 2]);
  await page.selectOption("#insert", { label: "Kicker" });
  check(await reads('kicker text role:kicker "Kicker" at:in(kicker)'), "a kicker inserted in the empty kicker slot fills it");
  check(await selects("kicker"), "and the canvas selects it");
  check(JSON.stringify(await box("kicker")) === JSON.stringify(kicker), `it stands in the slot: ${await box("kicker")}`);
  const scn = await source();
  check(scn.indexOf("kicker text") > scn.indexOf("state revenue") && scn.indexOf("kicker text") < scn.indexOf("state mix"), "it enters in `revenue`");
  const typing = await page
    .waitForFunction(() => window.scaena.canvas.typing() === "kicker" && window.scaena.canvas.typed(), null, { timeout: 10000 })
    .then((h) => h.jsonValue(), () => undefined);
  check(typing?.from === 0 && typing.to === "Kicker".length, `it is typed in at once, its words selected: ${JSON.stringify(typing)}`);
  await page.keyboard.type("Q3 at a glance");
  check(await reads('kicker text role:kicker "Q3 at a glance"'), "what is typed takes their place");
  check((await page.evaluate(() => window.scaena.canvas.armed())) === undefined && !(await status()).includes("Drawing"), `no key is the canvas's own: ${await status()}`);
  await page.keyboard.press("Control+z");
  check(await back(scn), "one undo takes the typing back");
  await page.keyboard.press("Control+z");
  check(await back(original), "and one more the insert");

  // A headline pressed over the chart, whose slot is filled: the chart is there, so it takes the
  // grid's cells in the room nearest the point, clear of what lint judges it against (PLAN 2.79).
  const at = [1400, 300];
  await press(at);
  await page.selectOption("#insert", { label: "Headline" });
  check(await reads("headline text role:headline"), "the headline is inserted into the source");
  check((await source()).includes('headline text role:headline "Headline" at:col('), "on the grid's cells");
  const cell = await drawn("headline");
  const overlaps = await page.evaluate(() => window.scaena.last().findings.filter((f) => f.code === "E101" && f.message.includes("`headline`")).map((f) => f.message));
  const on = Boolean(cell) && cell[0] <= at[0] && at[0] <= cell[0] + cell[2] && cell[1] <= at[1] && at[1] <= cell[1] + cell[3];
  check(Boolean(cell) && !on && overlaps.length === 0, `off the chart, in the room nearest the point: ${cell} · ${overlaps.join("; ") || "no overlap lint finds"}`);

  // Inserted, then deleted: nothing is left behind. Typed in at once, Escape leaves it selected.
  await page.waitForFunction(() => window.scaena.canvas.selected() === "headline", null, { timeout: 30000 }).catch(() => {});
  await page.waitForFunction(() => window.scaena.canvas.typing() === "headline", null, { timeout: 10000 }).catch(() => {});
  await page.keyboard.press("Escape");
  await page.locator("#overlay").focus();
  await page.keyboard.press("Delete");
  check(await back(original), "Delete takes a node just inserted out of the deck: the source is as it was");
  check(await says("deleted from every slide"), `and says so: ${await status()}`);

  // The chart deleted from `revenue` on, a chart of the source pressed into the figure's place
  // fills it, as a placeholder takes what is put on it: placed in the slot, at its size.
  await select("rev");
  await page.keyboard.press("Delete");
  check(await reads("\n  -rev"), "the chart deleted from `revenue` on");
  // Pressed once the canvas draws the slide without it, so the press lands on the place it left.
  await page.waitForFunction(() => !window.scaena.canvas.busy() && !window.scaena.canvas.boxes()?.some((b) => b.node === "rev"), null, { timeout: 30000 }).catch(() => {});
  const main = slots.main;
  await press([main[0] + main[2] / 2, main[1] + main[3] / 2]);
  // The source named as a person names it (PLAN 3.18): by its file, `data/q3-revenue.csv`.
  await page.selectOption("#insert", { label: "Q3 Revenue" });
  check(await reads("q3-chart chart:bar data:@q3"), "a chart of q3 is inserted");
  // Its declaration, with the lines its properties run on to.
  const q3 = /q3-chart chart:bar data:@q3[^\n]*(?:\n {4}[^\n]*)*/.exec(await source())?.[0] ?? "";
  check(q3.includes("at:in(main)"), `into the figure's place: ${q3.slice(-60)}`);
  const placed = await drawn("q3-chart");
  check(JSON.stringify(placed) === JSON.stringify(main), `at its size: ${placed}`);
  await page.keyboard.press("Control+z");
  await page.keyboard.press("Control+z");
  check(await back(original), "two undos put the chart back");
  await page.waitForTimeout(500);

  // The title deleted in `mix`: it leaves `mix`, and `close` shows it again by its own delta.
  await showState("mix", 2);
  await select("title");
  await page.keyboard.press("Delete");
  check(await reads("state mix slide:revenue transition:slow hold:6s\n  -title"), "Delete takes the title out of `mix`");
  check(!(await source()).includes("…and the mix shifted"), "with its delta there");
  check((await source()).includes('title "Thank you"'), "`close` shows it again by its own");
  check(await says("deleted from this slide on"), `the status says where from: ${await status()}`);
  check((await selected()) === undefined, "nothing is selected after");
  await page.keyboard.press("Control+z");
  check(await back(original), "one undo puts it back");

  // ⌘D copies the title beside it; Shift+Delete takes the copy out of the deck.
  await showState("revenue", 1);
  await select("title");
  await page.keyboard.press("Control+d");
  check(await reads("title-2 text"), "Ctrl+D copies the title as `title-2`");
  check(await says("duplicated"), `the status says so: ${await status()}`);
  check(await selects("title-2"), "and the canvas selects the copy");
  const [title, copy] = [await box("title"), await box("title-2")];
  check(Boolean(title && copy) && (copy[0] >= title[0] + title[2] || copy[1] >= title[1] + title[3]), `beside the title, clear of it: ${title} / ${copy}`);
  await page.keyboard.press("Shift+Delete");
  check(await back(original), "Shift+Delete takes the copy out of the deck");

  // The shape library (PLAN 2.106): a star, a polygon of ten points filling its box, in the
  // accent, named in a person's words.
  await press([700, 760]);
  await page.selectOption("#insert", { label: "Star" });
  check(await reads("star shape kind:polygon"), "a star is inserted as a polygon");
  // Its declaration, the lines its properties run on to, indented under it, read as one.
  const lines = (await source()).split("\n");
  const line = lines.findIndex((l) => l.trimStart().startsWith("star shape"));
  const indent = (l) => l.length - l.trimStart().length;
  const rest = line < 0 ? [] : lines.slice(line + 1);
  const end = rest.findIndex((l) => indent(l) <= indent(lines[line]));
  const star = line < 0 ? "" : [lines[line], ...rest.slice(0, end < 0 ? rest.length : end)].map((l) => l.trim()).join(" ");
  const corners = (/points:\[\[(.*?)\]\]/.exec(star)?.[1] ?? "").split("], [").length;
  check(corners === 10 && star.includes("fill:accent"), `its ten points, in the accent: ${star.slice(0, 90)}…`);
  check(await says("Star inserted on this slide"), `the status names it: ${await status()}`);
  // The inspector offers what a star draws: its fill, and no corners or shadow, which a polygon
  // does not draw; and heads it a Star.
  await page.waitForFunction(() => window.scaena.look.offered()?.node === "star", null, { timeout: 30000 }).catch(() => {});
  const starred = await page.evaluate(() => ({
    heading: document.querySelector("#look h2")?.textContent,
    fill: Boolean(document.querySelector("#look-fill")),
    radius: Boolean(document.querySelector("#look-radius")),
    shadow: Boolean(document.querySelector("#look-shadow")),
  }));
  check(starred.heading === "Star" && starred.fill && !starred.radius && !starred.shadow, `a star's inspector: ${JSON.stringify(starred)}`);
  // The layers call it a Star, as the menu does.
  const starLayer = await page
    .waitForFunction(() => document.querySelector('#layers li[data-layer="star"] [data-pick]')?.textContent, null, { timeout: 10000 })
    .then((h) => h.jsonValue(), () => undefined);
  check(starLayer === "Star", `the layers call it a Star: ${starLayer}`);
  await page.keyboard.press("Control+z");
  check(await back(original), "one undo takes it back");

  // A shader preset fills the canvas, under the rest.
  // Offered by its name, Backdrop; the node is named after the preset, `backdrop`.
  const preset = offered["Background Effect"]?.[0] ?? "Backdrop";
  const named = preset.toLowerCase();
  await page.selectOption("#insert", { label: preset });
  check(await reads(`${named} shader:`), `the ${preset} preset is inserted`);
  const filled = await drawn(named);
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
    const id = (await selects(`image-${hash}`)) ? `image-${hash}` : await selected();
    check(id === `image-${hash}` && Boolean(await box(id)), `it stands on the canvas, selected: ${id}`);
    // In the picture's own proportions, read from its header, and a corner dragged keeps the
    // proportions its box has, as a presentation app scales a picture.
    const shape = png.readUInt32BE(16) / png.readUInt32BE(20);
    const ratio = (r) => (r ? r[2] / r[3] : NaN);
    const inserted = await box(id);
    check(Math.abs(ratio(inserted) / shape - 1) <= 0.05, `it keeps the picture's proportions, ${shape.toFixed(3)}: ${inserted}`);
    await page.waitForFunction(() => document.querySelectorAll("#overlay rect.handle").length === 8, null, { timeout: 30000 }).catch(() => {});
    const corner = await page.evaluate(() => {
      const r = document.querySelector('#overlay rect.handle[data-edge="se"]');
      const at = (name) => Number(r?.getAttribute(name) ?? NaN);
      return [at("x") + at("width") / 2, at("y") + at("height") / 2];
    });
    const [from, to] = [await clientOf(corner), await clientOf([corner[0] - 150, corner[1] - 60])];
    const before = await source();
    await page.mouse.move(...from);
    await page.mouse.down();
    for (let k = 1; k <= 8; k++) await page.mouse.move(from[0] + ((to[0] - from[0]) * k) / 8, from[1] + ((to[1] - from[1]) * k) / 8);
    await page.waitForTimeout(400);
    await page.mouse.up();
    await page.waitForFunction((s) => window.scaena.source() !== s && !window.scaena.canvas.busy(), before, { timeout: 30000 }).catch(() => {});
    await page.waitForTimeout(300);
    const scaled = await box(id);
    check(
      Boolean(scaled) && scaled[2] < inserted[2] && Math.abs(ratio(scaled) / ratio(inserted) - 1) <= 0.02,
      `a corner dragged scales it in the proportions it has: ${scaled}`,
    );
    await page.keyboard.press("Control+z");
    // Let go a few units past another box's edge, within a guide's reach, its corner stays where it
    // was let go, in its proportions: no guide draws its right edge alone onto that edge, out of them.
    await page
      .waitForFunction(
        ([n, r]) => !window.scaena.canvas.busy() && JSON.stringify(window.scaena.canvas.boxes()?.find((b) => b.node === n)?.rect) === r,
        [id, JSON.stringify(inserted)],
        { timeout: 30000 },
      )
      .catch(() => {});
    const [x0, , w0, h0] = inserted;
    const PAST = 6;
    const stops = await page.evaluate((n) => {
      const [cw] = window.scaena.canvas.size();
      const rects = window.scaena.canvas.boxes().filter((b) => b.node !== n && b.draws).map((b) => b.rect);
      return [0, cw / 2, cw, ...rects.flatMap(([x, , w]) => [x, x + w / 2, x + w])];
    }, id);
    // An edge no other lies near, the nearest a drag of its corner can bring its right edge to,
    // in or out: the canvas's middle, or another box's edge or middle.
    const lone = (s) => stops.every((t) => t === s || Math.abs(t - s - PAST) > 16);
    const right = x0 + w0;
    const edge = stops
      .filter((s) => Math.abs(s + PAST - right) > 24 && s + PAST > x0 + 0.3 * w0 && s + PAST < x0 + 1.5 * w0 && lone(s))
      .sort((a, b) => Math.abs(a - right) - Math.abs(b - right))[0];
    if (edge === undefined) check(false, `an edge to let the picture's corner go beside: ${stops.join(", ")}`);
    else {
      // Across more than down, so its width leads and its height follows.
      const across = edge + PAST - right;
      const [at, past] = [await clientOf(corner), await clientOf([corner[0] + across, corner[1] + (across * h0) / w0 / 2])];
      const was = await source();
      await page.mouse.move(...at);
      await page.mouse.down();
      for (let k = 1; k <= 8; k++) await page.mouse.move(at[0] + ((past[0] - at[0]) * k) / 8, at[1] + ((past[1] - at[1]) * k) / 8);
      await page.waitForTimeout(400);
      await page.mouse.up();
      await page.waitForFunction((s) => window.scaena.source() !== s && !window.scaena.canvas.busy(), was, { timeout: 30000 }).catch(() => {});
      await page.waitForTimeout(300);
      const beside = await box(id);
      check(
        Boolean(beside) && Math.abs(beside[0] + beside[2] - (edge + PAST)) <= 2.5 && Math.abs(ratio(beside) / ratio(inserted) - 1) <= 0.01,
        `let go ${PAST} past an edge at ${edge}, it stays there in its proportions: ${beside}`,
      );
      await page.keyboard.press("Control+z");
    }
    await page.locator("#overlay").focus();
    await page.keyboard.press("Delete");
    check(await back(original), "Delete takes it out again");
  }

  // Insert › Image… (the QA pass's J3): a picture picked from the disk lands as one dropped does,
  // its path its `src`, one patch, one undo; the palette offers it too.
  const offeredImage = await page.evaluate(() => [...document.querySelectorAll("#insert > option")].map((o) => o.textContent));
  check(offeredImage.includes("Image…"), `Insert offers Image…: ${offeredImage.join(", ")}`);
  const fromDisk = await source();
  await press([400, 300]);
  const [chooser] = await Promise.all([page.waitForEvent("filechooser", { timeout: 10000 }).catch(() => undefined), page.selectOption("#insert", { label: "Image…" })]);
  check(chooser !== undefined, "Image… opens a file picker");
  if (chooser) {
    await chooser.setFiles({ name: "board.png", mimeType: "image/png", buffer: png });
    await page.waitForFunction((s) => window.scaena.source() !== s && !window.scaena.canvas.busy(), fromDisk, { timeout: 30000 }).catch(() => {});
    check(await reads(JSON.stringify(path)), `the picture picked is inserted, its path its \`src\`: ${await status()}`);
    check((await status()).includes("board.png"), `the status names it by the file picked: ${await status()}`);
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+z");
    check(await back(fromDisk), "one undo takes it out");
  }
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+k");
  await page.waitForFunction(() => document.querySelector("#palette").open, null, { timeout: 10000 }).catch(() => {});
  await page.keyboard.type("insert an image");
  const listed = await page.evaluate(() => window.scaena.commands.shown());
  check(JSON.stringify(listed).includes("Insert an image…"), `the palette offers Insert an image…: ${JSON.stringify(listed).slice(0, 200)}`);
  await page.keyboard.press("Escape");
  check(await page.evaluate(() => window.scaena.last().valid), "the source still compiles and validates");
} finally {
  await browser.close();
  await server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "nodes are inserted, copied, and deleted, each a patch");
process.exit(failures.length ? 1 : 0);

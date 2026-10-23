// PLAN 2.31 canvas check: the editor's preview as a canvas, in headless Chromium (serve.mjs), the
// CPU painting, as in web/editor.mjs.
//
//   node web/canvas.mjs     (after `just web`; from the repository's root)
//
// On the revenue example:
// - A click selects what the engine says is topmost there, and the inspector marks it; Escape
//   selects what holds it, and nothing past a node on the grid.
// - A drag into another slot moves the node's layer as it goes and says which states the move
//   changes before it is made. On drop, the source takes the patch as one change: the node stands
//   in its new slot, one undo puts the source back as it was, and one redo makes it again.
// - A drag pressed while the worker is busy still moves the node: the moves made before the
//   engine says what was pressed start it.
// - With Alt, the move is kept to the state shown, and the other states keep the node where it was.
// - With Shift, the node goes off the grid, placed by a `rect`, and the inspector flags it.
// - A click on the desk around the slide selects nothing, the inspector showing the slide's
//   settings; and a Shift-drag after a click on the page's words selects none of them.
// On the torture deck's `containers` case:
// - The arrow keys move a node a track, Shift with an arrow resizes it a track, and an arrow moves
//   a stack's child a place along it.
// - A handle, which stands clear of the box it resizes, resizes a grid container by tracks, and a
//   pause shows it laid out before the drop.
// Exits 1 on any failure.
import { mkdir, writeFile } from "node:fs/promises";
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
  const open = async (bundle) => {
    const page = await context.newPage();
    page.on("pageerror", (e) => failures.push(`${bundle}: ${e.message}`));
    page.on("console", (m) => m.type() === "error" && failures.push(`${bundle}: console: ${m.text()}`));
    await page.goto(url(bundle));
    await page.waitForFunction(
      () => window.scaena?.last() || document.querySelector("#status")?.textContent.startsWith("Error"),
      null,
      { timeout: 120000 },
    );
    const status = await page.evaluate(() => (window.scaena?.last() ? "" : document.querySelector("#status").textContent));
    if (status) throw new Error(`${bundle}: ${status}`);
    return page;
  };
  const trips = (page) => page.evaluate(() => window.scaena.trips().length);
  /** Once lint has answered for the source the editor holds after `before` trips. */
  const settled = (page, before) =>
    page.waitForFunction((n) => window.scaena.trips().length > n && !window.scaena.canvas.busy(), before, {
      timeout: 60000,
      polling: 50,
    });
  const source = (page) => page.evaluate(() => window.scaena.source());
  const status = (page) => page.evaluate(() => document.querySelector("#status").textContent);
  const box = (page, node) => page.evaluate((n) => window.scaena.canvas.boxes().find((b) => b.node === n)?.rect, node);
  /** Where the canvas draws the handle at `edge` of the node selected, canvas units: on its frame,
   * clear of its box. */
  const handle = (page, edge) =>
    page.evaluate((e) => {
      const r = document.querySelector(`#overlay rect.handle[data-edge="${e}"]`);
      const at = (name) => Number(r?.getAttribute(name) ?? NaN);
      return [at("x") + at("width") / 2, at("y") + at("height") / 2];
    }, edge);
  const center = ([x, y, w, h]) => [x + w / 2, y + h / 2];
  /** A point in canvas units, on the page. */
  const client = (page, [x, y]) =>
    page.evaluate(
      ([x, y]) => {
        const r = document.querySelector("#overlay").getBoundingClientRect();
        const [w, h] = window.scaena.canvas.size();
        return [r.left + (x / w) * r.width, r.top + (y / h) * r.height];
      },
      [x, y],
    );
  /** The preview's pixels, without the canvas's guides over them. */
  const shot = async (page) => {
    await page.evaluate(() => (document.querySelector("#overlay svg").style.visibility = "hidden"));
    const png = await page.locator("#stage").screenshot();
    await page.evaluate(() => (document.querySelector("#overlay svg").style.visibility = ""));
    return png;
  };
  /** The frame the worker committed to the preview's canvas, read from the canvas itself: its
   * SHA-256, or its PNG with `png`. */
  const committed = (page) =>
    page.evaluate(() => {
      window.committed = async (png) => {
        const stage = document.querySelector("#stage");
        const copy = new OffscreenCanvas(stage.width, stage.height);
        copy.getContext("2d").drawImage(stage, 0, 0);
        if (png) return [...new Uint8Array(await (await copy.convertToBlob()).arrayBuffer())];
        const pixels = copy.getContext("2d").getImageData(0, 0, copy.width, copy.height).data;
        return [...new Uint8Array(await crypto.subtle.digest("SHA-256", pixels))].map((b) => b.toString(16).padStart(2, "0")).join("");
      };
    });
  const frame = (page, png = false) => page.evaluate((png) => window.committed(png), png);
  /** How the inspector says `node` is placed in the state shown, once it says `expected`. */
  const placedAs = async (page, node, expected) => {
    const read = (n) => document.querySelector(`#inspector tr[data-node="${n}"]`)?.children[1]?.textContent ?? "";
    await page
      .waitForFunction(([n, e]) => (document.querySelector(`#inspector tr[data-node="${n}"]`)?.children[1]?.textContent ?? "").includes(e), [node, expected], { timeout: 30000 })
      .catch(() => {});
    return page.evaluate(read, node);
  };
  const click = async (page, at) => {
    const [x, y] = await client(page, at);
    await page.mouse.click(x, y);
  };
  /** Show the state declared as `state` in the source. */
  const showState = async (page, state, index) => {
    const at = (await source(page)).indexOf(`state ${state}`) + "state ".length;
    await page.evaluate((offset) => window.scaena.cursor(offset), at);
    await page.waitForFunction((i) => window.scaena.shown() === i && window.scaena.at().index === i, index, { timeout: 30000 });
    // The listing names the slide shown as a person counts it (PLAN 2.99).
    await page.waitForFunction(
      (i) => {
        const said = window.scaena.last()?.slots[i]?.said;
        return said !== undefined && (document.querySelector("#listing h2")?.textContent ?? "").split(" · ")[0] === said;
      },
      index,
      { timeout: 30000 },
    );
    // The canvas's boxes are the shown state's, not the one shown before it.
    await page.waitForFunction((s) => window.scaena.canvas.boxed() === s && window.scaena.canvas.boxes().length > 0, state, { timeout: 30000 });
  };
  /** Press at `from`, move past the slop and on to `to` with `keys` held, and wait until the
   * status says where it lands; then let go, unless `hold`. The status as it was then. */
  const drag = async (page, from, to, keys = [], { hold = false, says = "→" } = {}) => {
    const [fx, fy] = await client(page, from);
    const [tx, ty] = await client(page, to);
    await page.mouse.move(fx, fy);
    await page.mouse.down();
    for (const key of keys) await page.keyboard.down(key);
    await page.mouse.move(fx + 8, fy + 8, { steps: 2 });
    await page.mouse.move(tx, ty, { steps: 8 });
    await page.waitForFunction((s) => document.querySelector("#status").textContent.includes(s), says, { timeout: 30000 }).catch(() => {});
    // The last move's request answered: what the status says is for where the pointer is now, not
    // a place it has left.
    await page.waitForFunction(() => window.scaena.canvas.answered(), null, { timeout: 30000 }).catch(() => {});
    const said = await status(page);
    if (!hold) {
      await page.mouse.up();
      for (const key of keys) await page.keyboard.up(key);
    }
    return said;
  };

  // The revenue example: `note` in its slot, in `revenue` and `mix`.
  const page = await open("/docs/examples/revenue.deck.json");
  await committed(page);
  await showState(page, "revenue", 1);
  const original = await source(page);
  const note = await box(page, "note");
  check(Boolean(note), "the canvas knows where the note stands");

  // A click selects; Escape selects what holds it, and nothing past a node on the grid.
  await click(page, center(note));
  await page.waitForFunction(() => window.scaena.canvas.selected() === "note", null, { timeout: 30000 }).catch(() => {});
  check((await page.evaluate(() => window.scaena.canvas.selected())) === "note", "a click on the note selects it");
  check(
    (await page.evaluate(() => document.querySelector('#inspector tr[aria-selected="true"]')?.getAttribute("data-node"))) === "note",
    "the inspector marks the node selected",
  );
  check((await page.locator("#overlay rect.selected").count()) === 1, "the canvas draws the selection");
  await page.keyboard.press("Escape");
  check((await page.evaluate(() => window.scaena.canvas.selected())) === undefined, "Escape on a node on the grid selects nothing");

  // A drag into the kicker slot.
  const targets = await page.evaluate(() => window.scaena.canvas.targets("note"));
  const kicker = targets.slots?.kicker;
  check(targets.by === "grid" && Boolean(kicker), `the note's targets are the grid's slots: ${Object.keys(targets.slots ?? {})}`);
  const before = await shot(page);
  const rest = await frame(page);
  await click(page, center(note));
  let n = await trips(page);
  const shift = [center(kicker)[0] - center(targets.cell)[0], center(kicker)[1] - center(targets.cell)[1]];
  const to = [center(note)[0] + shift[0], center(note)[1] + shift[1]];
  const midway = await drag(page, center(note), to, [], { hold: true, says: "in the Kicker slot" });
  // The canvas's own frame, not a screenshot: on a loaded runner the compositor's surface can lag
  // what the canvas holds (#448's run 39922562592 shot the rest frame mid-drag).
  const drawn = await page
    .waitForFunction(async (r) => (await window.committed()) !== r, rest, { timeout: 30000 })
    .then(() => true, () => false);
  // The screenshot too, kept with the canvas's frame where they disagree: a recurrence shows which.
  const moving = await shot(page);
  if (!drawn || moving.equals(before)) {
    await mkdir("target/web-smoke", { recursive: true });
    await writeFile("target/web-smoke/canvas-moving-shot.png", moving);
    await writeFile("target/web-smoke/canvas-moving-frame.png", Buffer.from(await frame(page, true)));
  }
  await page.mouse.up();
  check(midway.includes("in the Kicker slot") && midway.includes("on 2 steps of Slide 2"), `before the drop, the status says where and on which slides: ${midway}`);
  check(drawn, "while it moves, the note's layer is drawn where it is dragged");
  await settled(page, n);
  const placed = await source(page);
  check(placed !== original, "the drop changes the source");
  check((await placedAs(page, "note", "in the Kicker slot")).includes("in the Kicker slot"), "the inspector says the note is in the kicker slot");
  check(!(await shot(page)).equals(before), "the preview shows it there");
  check((await page.evaluate(() => window.scaena.last().valid)), "the source after the drop compiles and validates");

  // One step to undo, one to redo, from the canvas.
  n = await trips(page);
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+z");
  await settled(page, n);
  check((await source(page)) === original, "one undo puts the source back as it was");
  check((await shot(page)).equals(before), "and the preview with it");
  n = await trips(page);
  await page.keyboard.press("Control+Shift+z");
  await settled(page, n);
  check((await source(page)) === placed, "one redo makes the move again");
  n = await trips(page);
  await page.keyboard.press("Control+z");
  await settled(page, n);

  // A drag pressed while the worker is busy, as it is while it paints a frame or lints the deck:
  // the moves made before the engine says what was pressed still drag the node, and it drops.
  await click(page, center(note));
  n = await trips(page);
  const early = await page.evaluate(() => window.scaena.canvas.early());
  // The press and its move past the slop go to the canvas in the page's same task as the requests
  // that keep the worker busy, so both are in before it can answer the press, however loaded the
  // machine; the browser's own input could come after the answer.
  const [[fx, fy], [tx, ty]] = [await client(page, center(note)), await client(page, to)];
  const pointer = (moves, busy = false) =>
    page.evaluate(
      ([moves, busy]) => {
        if (busy) for (let i = 0; i < 3000; i++) void window.scaena.canvas.targets("note");
        for (const [type, x, y] of moves) {
          const buttons = type === "pointerup" ? 0 : 1;
          const e = { bubbles: true, cancelable: true, pointerId: 1, pointerType: "mouse", isPrimary: true, button: 0, buttons, clientX: x, clientY: y };
          document.querySelector("#overlay").dispatchEvent(new PointerEvent(type, e));
        }
      },
      [moves, busy],
    );
  await pointer([["pointerdown", fx, fy], ["pointermove", fx + 8, fy + 8]], true);
  await pointer([["pointermove", tx, ty]]);
  await page.waitForFunction(() => document.querySelector("#status").textContent.includes("in the Kicker slot"), null, { timeout: 30000 }).catch(() => {});
  await page.waitForFunction(() => window.scaena.canvas.answered(), null, { timeout: 30000 }).catch(() => {});
  const late = await status(page);
  await pointer([["pointerup", tx, ty]]);
  check(late.includes("in the Kicker slot"), `a drag pressed while the worker is busy moves the note: ${late}`);
  check((await page.evaluate(() => window.scaena.canvas.early())) > early, "it began from the moves made before the engine answered");
  await settled(page, n);
  check((await placedAs(page, "note", "in the Kicker slot")).includes("in the Kicker slot"), "and it drops in the kicker slot");
  n = await trips(page);
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+z");
  await settled(page, n);
  check((await source(page)) === original, "one undo puts it back");

  // Alt keeps the move to the state shown: in `mix`, `revenue` keeps the note in its slot.
  await showState(page, "mix", 2);
  const mixNote = await box(page, "note");
  await click(page, center(mixNote));
  n = await trips(page);
  const kept = await drag(page, center(mixNote), [center(mixNote)[0] + shift[0], center(mixNote)[1] + shift[1]], ["Alt"], { says: "on this slide" });
  check(kept.includes("on this slide") && kept.includes("kept to this slide"), `with Alt, the move is kept to the slide shown: ${kept}`);
  await settled(page, n);
  check((await placedAs(page, "note", "in the Kicker slot")).includes("in the Kicker slot"), "in `mix`, the note is in the kicker slot");
  await showState(page, "revenue", 1);
  check((await placedAs(page, "note", "in the Note slot")).includes("in the Note slot"), "in `revenue`, it stays in its own");
  n = await trips(page);
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+z");
  await settled(page, n);
  check((await source(page)) === original, "and one undo puts it back");

  // Shift takes it off the grid: a `rect`, which the inspector flags. (The undo put the cursor
  // back in `mix`, where the move it undid was made.)
  await showState(page, "revenue", 1);
  await click(page, center(note));
  n = await trips(page);
  const off = await drag(page, center(note), [center(note)[0] + 37, center(note)[1] - 53], ["Shift"], { says: "off the grid" });
  check(off.includes("off the grid"), `with Shift, it goes off the grid: ${off}`);
  await settled(page, n);
  const flagged = await placedAs(page, "note", "off the grid");
  check(/^at [\d.]+, [\d.]+, [\d.]+ × [\d.]+ off the grid$/.test(flagged), `the inspector shows the box placed off the grid: ${flagged}`);
  check((await page.evaluate(() => window.scaena.last().findings.some((f) => f.code === "W301" && f.node === "note"))), "lint flags it (W301)");
  n = await trips(page);
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+z");
  await settled(page, n);

  // A press on the desk around the slide selects nothing, as a click off a slide does in a
  // presentation app: the inspector shows the slide's own settings.
  await click(page, center(note));
  await page.waitForFunction(() => window.scaena.canvas.selected() === "note" && !window.scaena.canvas.busy(), null, { timeout: 30000 }).catch(() => {});
  await page.waitForTimeout(300);
  const desk = await page.evaluate(() => {
    const r = document.querySelector("#preview").getBoundingClientRect();
    return [r.left - 24, r.top + r.height / 2];
  });
  await page.mouse.click(desk[0], desk[1]);
  const offSlide = await page
    .waitForFunction(() => window.scaena.canvas.selected() === undefined && document.querySelector('#look [data-prop^="transition/"]'), null, { timeout: 30000 })
    .then(() => true)
    .catch(() => false);
  check(offSlide, `a click on the desk selects nothing, and the inspector shows the slide's settings: ${await page.evaluate(() => window.scaena.canvas.selected())}`);
  check((await page.evaluate(() => document.activeElement?.id)) === "overlay", "and the canvas keeps the keys");

  // A Shift-drag on the canvas after a click on the page's words selects none of them: the canvas
  // takes the press.
  await page.click("#look h2");
  n = await trips(page);
  const [sx, sy] = await client(page, center(note));
  await page.keyboard.down("Shift");
  await page.mouse.move(sx, sy);
  await page.mouse.down();
  await page.mouse.move(sx + 10, sy + 10, { steps: 2 });
  await page.mouse.move(sx + 40, sy + 60, { steps: 6 });
  await page.waitForTimeout(300);
  const picked = await page.evaluate(() => getSelection()?.toString() ?? "");
  await page.mouse.up();
  await page.keyboard.up("Shift");
  check(picked === "", `a Shift-drag selects none of the page's words: ${JSON.stringify(picked.slice(0, 60))}`);
  await settled(page, n).catch(() => {});
  if ((await source(page)) !== original) {
    n = await trips(page);
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+z");
    await settled(page, n);
  }
  await page.close();

  // The torture deck's containers: the arrow keys, a stack's order, a handle.
  const torture = await open("/tests/fixtures/torture.scaena/deck.json");
  const index = await torture.evaluate(() => window.scaena.opened.states.indexOf("containers"));
  await showState(torture, "containers", index);
  // A click on the tally's label selects the label; Escape, the tally that holds it.
  await click(torture, center(await box(torture, "tally-label")));
  await torture.waitForFunction(() => window.scaena.canvas.selected() === "tally-label", null, { timeout: 30000 }).catch(() => {});
  await torture.keyboard.press("Escape");
  check((await torture.evaluate(() => window.scaena.canvas.selected())) === "tally", "Escape selects what holds the node selected");
  const start = await placedAs(torture, "tally", "column");
  n = await trips(torture);
  await torture.keyboard.press("ArrowRight");
  await settled(torture, n);
  check((await placedAs(torture, "tally", "columns 2–12")).includes("columns 2–12"), `an arrow moves it a track: ${start} → ${await placedAs(torture, "tally", "columns 2–12")}`);
  n = await trips(torture);
  await torture.keyboard.press("Shift+ArrowLeft");
  await settled(torture, n);
  check((await placedAs(torture, "tally", "columns 2–11")).includes("columns 2–11"), "Shift with an arrow resizes it a track");
  // Two presses at once: each is made on the source the one before left.
  await torture.keyboard.press("Shift+ArrowLeft");
  await torture.keyboard.press("Shift+ArrowLeft");
  check((await placedAs(torture, "tally", "columns 2–9")).includes("columns 2–9"), "two presses in a row make two tracks");
  await torture.waitForFunction(() => !window.scaena.canvas.busy(), null, { timeout: 30000 });

  // A stack's child, a place along it.
  await click(torture, center(await box(torture, "stat-a-figure")));
  await torture.waitForFunction(() => window.scaena.canvas.selected() === "stat-a-figure", null, { timeout: 30000 }).catch(() => {});
  await torture.keyboard.press("Escape");
  check((await torture.evaluate(() => window.scaena.canvas.selected())) === "stat-a", "Escape selects the stack's child that holds it");
  n = await trips(torture);
  await torture.keyboard.press("ArrowRight");
  await settled(torture, n);
  await torture.waitForFunction(() => {
    const at = (id) => window.scaena.canvas.boxes().find((b) => b.node === id)?.rect[0] ?? 0;
    return at("stat-a") > at("stat-b");
  }, null, { timeout: 30000 }).catch(() => {});
  const order = await torture.evaluate(() => ["stat-a", "stat-b"].map((id) => window.scaena.canvas.boxes().find((b) => b.node === id)?.rect[0]));
  check(order[0] > order[1], `an arrow moves a stack's child a place along it: stat-a at ${order[0]}, stat-b at ${order[1]}`);

  // A handle: the board, a grid container on the theme's grid, a track narrower.
  await click(torture, center(await box(torture, "board-dot")));
  await torture.waitForFunction(() => window.scaena.canvas.selected() === "board-dot", null, { timeout: 30000 }).catch(() => {});
  await torture.keyboard.press("Escape");
  check((await torture.evaluate(() => window.scaena.canvas.selected())) === "board", "Escape selects the grid that holds the dot");
  await torture.waitForFunction(() => document.querySelectorAll("#overlay rect.handle").length === 8, null, { timeout: 30000 }).catch(() => {});
  check((await torture.locator("#overlay rect.handle").count()) === 8, "a node placed by cells has handles");
  const board = await torture.evaluate(() => window.scaena.canvas.targets("board"));
  const pitch = board.columns[1][0] - board.columns[0][0];
  const [bx, by, bw, bh] = await box(torture, "board");
  const [ex, ey] = await handle(torture, "e");
  check(ex > bx + bw && Math.abs(ey - (by + bh / 2)) < 0.01, `the handle at its right edge stands clear of its box: ${ex} past ${bx + bw}`);
  const still = await shot(torture);
  n = await trips(torture);
  const narrower = await drag(torture, [ex, ey], [ex - pitch, ey], [], { hold: true, says: "columns 1–7" });
  // Paused, the preview shows it laid out as the patch would make it: once the engine has laid it
  // out and painted it, which a busy machine takes longer to do than an idle one.
  await torture.waitForTimeout(900);
  let paused = await shot(torture);
  for (let k = 0; k < 40 && paused.equals(still); k++) {
    await torture.waitForTimeout(250);
    paused = await shot(torture);
  }
  await torture.mouse.up();
  check(narrower.includes("columns 1–7"), `a handle resizes it by tracks: ${narrower}`);
  check(!paused.equals(still), "when the resize pauses, the preview shows it laid out");
  await settled(torture, n);
  check((await placedAs(torture, "board", "columns 1–7")).includes("columns 1–7"), "on drop, the board is a track narrower");
  check((await torture.evaluate(() => window.scaena.last().valid)), "the torture deck still validates");
} catch (e) {
  failures.push(String(e));
} finally {
  await browser.close();
  server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "the canvas selects, moves, resizes, and undoes");
process.exit(failures.length ? 1 : 0);

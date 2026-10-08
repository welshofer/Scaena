// PLAN 2.51 check: a node's transform on the canvas, in headless Chromium (serve.mjs), the CPU
// painting, on the torture deck's `transforms` case.
//
//   node web/rotate.mjs     (after `just web`; from the repository's root)
//
// - A turned node's box carries the map its transform draws it through, and its selection is
//   drawn through it: an outline turned with it, its handles at its turned corners, and a rotate
//   handle above its top edge as drawn.
// - A click over the turned headline's raised end selects it; one over its box as laid out, where
//   the turn drew nothing of it, does not.
// - The rotate handle turns the node selected about its anchor: a drag a quarter of the way round
//   writes `transform/rotate` 90 (one `choose`, where it lives), and one undo takes it back. With
//   Shift it goes by 15°, and Escape leaves it as it was.
// - The inspector shows the angle, and one typed there turns the node.
// - A double click on a turned headline's words types in it where the click was, read back
//   through the turn, and the caret is drawn turned with it.
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
  const says = async (text) => {
    await page.waitForFunction((t) => document.querySelector("#status").textContent.includes(t), text, { timeout: 30000 }).catch(() => {});
    return (await status()).includes(text);
  };
  const back = async (to) => {
    await page.waitForFunction((s) => window.scaena.source() === s, to, { timeout: 30000, polling: 50 }).catch(() => {});
    await page.waitForTimeout(300);
    return (await source()) === to;
  };
  const undo = async () => {
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+z");
  };
  /** Once the canvas draws the pennant upright again, as the source an undo left says: a turn
   * pressed sooner starts from the angle the canvas still draws. */
  const upright = () =>
    page.waitForFunction(() => !window.scaena.canvas.boxes()?.find((b) => b.node === "tf-flag")?.transform, null, { timeout: 30000 }).catch(() => {});
  const box = (node) => page.evaluate((n) => window.scaena.canvas.boxes()?.find((b) => b.node === n), node);
  /** Where the source declares `node`: its line and the lines that go on from it. */
  const declared = (text, node) => {
    const at = text.indexOf(`\n  ${node} `);
    if (at < 0) return "";
    const rest = text.slice(at + 1);
    const end = rest.search(/\n {0,2}\S/);
    return end < 0 ? rest : rest.slice(0, end);
  };
  const apply = (m, [x, y]) => [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]];
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
  const click = async (at) => {
    const [x, y] = await client(at);
    await page.mouse.click(x, y);
  };
  const selected = () => page.evaluate(() => window.scaena.canvas.selected());
  const selects = async (node) => {
    await page.waitForFunction((n) => window.scaena.canvas.selected() === n, node, { timeout: 30000 }).catch(() => {});
    return (await selected()) === node;
  };
  /** The rotate handle's middle, canvas units, once it is drawn. */
  const handle = async () => {
    await page.waitForSelector("#overlay svg [data-turn]", { timeout: 30000 }).catch(() => {});
    return page.evaluate(() => {
      const c = document.querySelector("#overlay svg [data-turn]");
      return c ? [Number(c.getAttribute("cx")), Number(c.getAttribute("cy"))] : undefined;
    });
  };
  /** Press the rotate handle and go `degrees` round `pivot`, a step at a time, with `keys` held;
   * let go unless `hold`. */
  const turn = async (pivot, degrees, keys = [], { hold = false } = {}) => {
    const from = await handle();
    const [dx, dy] = [from[0] - pivot[0], from[1] - pivot[1]];
    const [fx, fy] = await client(from);
    await page.mouse.move(fx, fy);
    await page.mouse.down();
    for (const key of keys) await page.keyboard.down(key);
    const steps = 12;
    for (let k = 1; k <= steps; k++) {
      const a = ((degrees * k) / steps) * (Math.PI / 180);
      const at = [pivot[0] + dx * Math.cos(a) - dy * Math.sin(a), pivot[1] + dx * Math.sin(a) + dy * Math.cos(a)];
      const [x, y] = await client(at);
      await page.mouse.move(x, y);
    }
    const said = await status();
    if (!hold) {
      await page.mouse.up();
      for (const key of keys) await page.keyboard.up(key);
    }
    return said;
  };

  // The torture deck's `transforms` case.
  const shown = await page.evaluate(() => window.scaena.opened.states.indexOf("transforms"));
  await page.evaluate(() => window.scaena.cursor(window.scaena.source().indexOf("state transforms ") + "state ".length));
  await page.waitForFunction((i) => window.scaena.shown() === i && window.scaena.canvas.boxed() === "transforms", shown, { timeout: 30000 });
  const original = await source();

  // The headline, turned 6° anticlockwise: its box carries its map.
  const title = await box("tf-title");
  const m = title?.transform;
  check(Array.isArray(m) && Math.abs(m[1] - Math.sin((-6 * Math.PI) / 180)) < 1e-4, `the headline's box carries its turn: ${JSON.stringify(m)}`);
  const [x, y, w] = title.rect;
  // Over its raised end, above its box as laid out: it is selected there.
  const raised = apply(m, [x + w - 12, y + 12]);
  check(raised[1] < y, `its right end is drawn above its box as laid out: ${raised.map(Math.round)}`);
  await click(raised);
  check(await selects("tf-title"), "a click over the turned headline's raised end selects it");
  check((await page.locator("#overlay svg polygon.selected").count()) === 1, "its selection is drawn turned with it");
  check((await page.locator("#overlay svg [data-turn]").count()) === 1, "with a rotate handle above it");
  // Over its box as laid out, at its top left, which the turn lowered: nothing of it is there.
  await page.keyboard.press("Escape");
  await click([x + 12, y + 12]);
  await page.waitForTimeout(500);
  check((await selected()) !== "tf-title", `a click where the turn drew nothing of it does not select it: ${await selected()}`);

  // The pennant, upright: a quarter turn by its handle.
  const flag = await box("tf-flag");
  check(flag && flag.transform === undefined, "the pennant stands as laid out");
  const [fx, fy, fw, fh] = flag.rect;
  const pivot = [fx + fw / 2, fy + fh / 2];
  await click(pivot);
  check(await selects("tf-flag"), "a click on the pennant selects it");
  await turn(pivot, 90);
  check(await says("tf-flag turned to 90°"), `a quarter turn by the handle turns it to 90°: ${await status()}`);
  const turned = await source();
  check(declared(turned, "tf-flag").includes("transform:{rotate: 90}"), `written where it lives, on the pennant: ${JSON.stringify(declared(turned, "tf-flag"))}`);
  await page.waitForFunction(() => window.scaena.canvas.boxes()?.find((b) => b.node === "tf-flag")?.transform, null, { timeout: 30000 }).catch(() => {});
  const now = (await box("tf-flag"))?.transform;
  check(Array.isArray(now) && Math.abs(now[1] - 1) < 1e-4, `its box carries the quarter turn: ${JSON.stringify(now)}`);
  await undo();
  check(await back(original), "one undo takes it back");
  await upright();

  // With Shift, by 15°: most of 50° is 45°.
  await click(pivot);
  await selects("tf-flag");
  await turn(pivot, 50, ["Shift"]);
  check(await says("tf-flag turned to 45°"), `with Shift it goes by 15°: ${await status()}`);
  check((await source()).includes("transform:{rotate: 45}"), "and writes 45");
  await undo();
  check(await back(original), "one undo takes it back");
  await upright();

  // Escape, mid-turn: as it was.
  await click(pivot);
  await selects("tf-flag");
  const mid = await turn(pivot, 30, [], { hold: true });
  check(mid.includes("tf-flag turns to 30°"), `the status follows the turn: ${mid}`);
  await page.keyboard.press("Escape");
  check(await says("tf-flag stays as it is"), `Escape leaves it as it was: ${await status()}`);
  await page.mouse.up();
  await page.waitForTimeout(500);
  check((await source()) === original, "and nothing changes");

  // The inspector's angle.
  await click(pivot);
  await selects("tf-flag");
  await page.locator("#tab-inspector").click().catch(() => {});
  await page.waitForFunction(() => window.scaena.look.offered()?.node === "tf-flag", null, { timeout: 30000 }).catch(() => {});
  await page.waitForSelector("#look-transform-rotate", { timeout: 30000 }).catch(() => {});
  check((await page.locator("#look-transform-rotate").count()) === 1, "the inspector shows the angle");
  await page.evaluate(() => {
    const input = document.querySelector("#look-transform-rotate");
    input.value = "30";
    input.dispatchEvent(new Event("change", { bubbles: true }));
  });
  check(await says("tf-flag's transform/rotate: 30°"), `one typed there turns it: ${await status()}`);
  check((await source()).includes("transform:{rotate: 30}"), "and writes 30");
  await undo();
  check(await back(original), "one undo takes it back");
  await upright();

  // Typing in a turned text: the headline turned a quarter by the inspector, then a double click
  // on its words where they are drawn, a fifth of the way along them: the caret goes there, read
  // back through the turn, which the point unread would miss by most of the canvas.
  await click(raised);
  await selects("tf-title");
  // The inspector shows the headline before its angle is typed: the pennant's angle stands there
  // until it does, and a value typed there turns the pennant.
  await page.waitForFunction(() => window.scaena.look.offered()?.node === "tf-title", null, { timeout: 30000 }).catch(() => {});
  await page.waitForSelector("#look-transform-rotate", { timeout: 30000 }).catch(() => {});
  await page.evaluate(() => {
    const input = document.querySelector("#look-transform-rotate");
    input.value = "90";
    input.dispatchEvent(new Event("change", { bubbles: true }));
  });
  check(await says("tf-title's transform/rotate: 90°"), `the inspector turns the headline a quarter: ${await status()}`);
  await page
    .waitForFunction(() => Math.abs((window.scaena.canvas.boxes()?.find((b) => b.node === "tf-title")?.transform?.[1] ?? 0) - 1) < 1e-4, null, { timeout: 30000 })
    .catch(() => {});
  const quarter = (await box("tf-title"))?.transform;
  check(Array.isArray(quarter) && Math.abs(quarter[1] - 1) < 1e-4, `its box carries the quarter turn: ${JSON.stringify(quarter)}`);
  const [, , , th] = title.rect;
  const along = apply(quarter, [x + 0.2 * w, y + 0.3 * th]);
  const [ax, ay] = await client(along);
  await page.mouse.dblclick(ax, ay);
  await page.waitForFunction(() => window.scaena.canvas.typing() === "tf-title", null, { timeout: 30000 }).catch(() => {});
  check((await page.evaluate(() => window.scaena.canvas.typing())) === "tf-title", "a double click on the turned headline's words types in it");
  const caret = await page.evaluate(() => window.scaena.canvas.typed());
  check(
    caret && caret.from === caret.to && caret.from >= 2 && caret.from <= 7,
    `the caret is where the click was, a fifth of the way along "Turned six degrees": ${JSON.stringify(caret)}`,
  );
  check((await page.locator("#overlay svg g[transform] rect.caret").count()) === 1, "the caret is drawn turned with the headline");
  await page.keyboard.press("Escape");
  await page.waitForFunction(() => window.scaena.canvas.typing() === undefined, null, { timeout: 30000 }).catch(() => {});
  await undo();
  check(await back(original), "one undo takes the turn back");
} finally {
  await browser.close();
  await server.close();
}

if (failures.length) {
  console.log(`\n${failures.length} failed`);
  process.exit(1);
}
console.log("\nturning: all passed");

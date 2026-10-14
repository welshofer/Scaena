// PLAN 2.46 check: the preview zoomed and panned, in headless Chromium (serve.mjs), the CPU
// painting, on the revenue example.
//
//   node web/zoom.mjs     (after `just web`; from the repository's root)
//
// - ⌘+ zooms a step about the middle of what is shown, painted at the size shown: the canvas keeps
//   its pixels and shows less of the deck in them. The control says how close.
// - Zoomed in, the grid and the outlines draw within the slide's frame, none of it on the desk.
// - A click selects what is under it through the view, and a drag moves it by what the pointer
//   moved, in canvas units through the view: twice as far on the screen twice as close lands
//   where the drag at the whole canvas did. One patch, one undo.
// - The wheel pans; ⌘ or Ctrl with the wheel zooms about the pointer, the point under it staying
//   put; Space and a drag pan, selecting and changing nothing.
// - With nothing selected, the arrow keys pan a tenth of what is shown, and with Shift half,
//   changing nothing; and Tab, zoomed in on the middle, selects each object in reading order and
//   brings it into view: the least move that shows it whole (PLAN 2.125).
// - A text is typed in where it stands zoomed in.
// - ⌘0 shows the whole canvas again, and another format does too. ⌘+ and ⌘0 zoom the slide from a
//   slide focused in the strip too, never the browser's page.
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
  const view = () => page.evaluate(() => window.scaena.canvas.view());
  const zoomed = () => page.evaluate(() => window.scaena.canvas.zoomed());
  const shows = () => page.evaluate(() => document.querySelector("#zoom output").textContent);
  const box = (node) => page.evaluate((n) => window.scaena.canvas.boxes().find((b) => b.node === n)?.rect, node);
  const settled = (before) =>
    page.waitForFunction((n) => window.scaena.trips().length > n && !window.scaena.canvas.busy(), before, { timeout: 60000, polling: 50 });
  /** Where canvas point `at` is on the page, through the view. */
  const client = async ([x, y]) => {
    const r = await page.locator("#overlay").boundingBox();
    const [vx, vy, vw, vh] = await view();
    return [r.x + ((x - vx) / vw) * r.width, r.y + ((y - vy) / vh) * r.height];
  };
  /** The canvas point under page point `at`, through the view. */
  const canvasAt = async ([px, py]) => {
    const r = await page.locator("#overlay").boundingBox();
    const [vx, vy, vw, vh] = await view();
    return [vx + ((px - r.x) / r.width) * vw, vy + ((py - r.y) / r.height) * vh];
  };
  /** The preview as painted: without the overlay's drawing, or its focus ring. */
  const shot = async () => {
    const hide = (hidden) => {
      const overlay = document.querySelector("#overlay");
      overlay.querySelector("svg").style.visibility = hidden ? "hidden" : "";
      overlay.style.outline = hidden ? "none" : "";
    };
    await page.evaluate(hide, true);
    const png = await page.locator("#stage").screenshot();
    await page.evaluate(hide, false);
    return png;
  };
  const near = (a, b, by = 1) => Math.abs(a - b) <= by;

  // Show `revenue`, where the chart is.
  const at = (await source()).indexOf("state revenue") + "state ".length;
  await page.evaluate((offset) => window.scaena.cursor(offset), at);
  await page.waitForFunction(() => window.scaena.canvas.boxed() === "revenue" && window.scaena.canvas.boxes().length > 0, null, { timeout: 30000 });
  const original = await source();
  const [W, H] = await page.evaluate(() => window.scaena.canvas.size());
  check((await zoomed()) === 1 && (await shows()) === "100%", `the whole canvas at first: ${await shows()}`);
  const whole = await shot();
  const pixels = await page.evaluate(() => [document.querySelector("#stage").width, document.querySelector("#stage").height]);

  /** Drag `node` from its middle `dx` page pixels across and half that down, off the grid (Shift),
   * by whole canvas units where the pointer says, onto no guide (Control: a guide's reach is in
   * pixels, so it differs with the zoom, PLAN 2.57); and say where it lands; then undo it. */
  const dragged = async (node, dx) => {
    const [x, y, w, h] = await box(node);
    const [sx, sy] = await client([x + w / 2, y + h / 2]);
    const n = await trips();
    await page.evaluate(() => (document.querySelector("#status").textContent = ""));
    await page.mouse.move(sx, sy);
    await page.mouse.down();
    await page.keyboard.down("Shift");
    await page.keyboard.down("Control");
    await page.mouse.move(sx + 8, sy + 8, { steps: 2 });
    await page.mouse.move(sx + dx, sy + dx / 2, { steps: 8 });
    // Where it lands, as the status says once the last move is answered.
    await page.waitForFunction(() => document.querySelector("#status").textContent.includes(" × "), null, { timeout: 30000 }).catch(() => {});
    await page.waitForTimeout(150);
    await page.mouse.up();
    await page.keyboard.up("Control");
    await page.keyboard.up("Shift");
    await settled(n);
    const landed = await box(node);
    await page.locator("#overlay").focus();
    const m = await trips();
    await page.keyboard.press("Control+z");
    await settled(m);
    return landed;
  };
  // A drag at the whole canvas, to hold the zoomed one to.
  const unit1 = W / (await page.locator("#overlay").boundingBox()).width;
  const d = Math.round(W / 12 / unit1);
  const atFit = await dragged("rev", d);
  check((await source()) === original, "a drag, undone, leaves the deck as it was");

  // ⌘+: a step closer, about the middle, painted at the size shown.
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+Equal");
  await page.waitForFunction(() => window.scaena.canvas.zoomed() > 1, null, { timeout: 10000 });
  const v1 = await view();
  check(near(v1[2], W / 1.5) && near(v1[0], (W - W / 1.5) / 2) && near(v1[1], (H - H / 1.5) / 2), `⌘+ shows the middle two thirds: ${v1.map(Math.round)}`);
  check((await shows()) === "150%", `and the control says so: ${await shows()}`);
  const [cx, cy, cw, ch] = await box("rev");
  await page.evaluate(([x, y]) => window.scaena.canvas.zoom(2, [x, y]), [cx + cw / 2, cy + ch / 2]);
  const zoomedShot = await shot();
  check(!zoomedShot.equals(whole), "the preview shows less of the deck, closer");
  const after = await page.evaluate(() => [document.querySelector("#stage").width, document.querySelector("#stage").height]);
  check(after[0] === pixels[0] && after[1] === pixels[1], `in the same pixels, painted at the size shown: ${after}`);

  // Zoomed in, the grid draws within the slide's frame, over no toolbar or timeline: the desk
  // beside the slide looks the same with the grid drawn as without it.
  const beside = async () => {
    const clip = await page.evaluate(() => {
      const p = document.querySelector("#preview").getBoundingClientRect();
      const x = Math.max(0, p.left - 48);
      return { x, y: p.top + 16, width: Math.min(32, p.left - 12 - x), height: p.height - 32 };
    });
    return page.screenshot({ clip });
  };
  const bare = await beside();
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+Quote");
  await page.waitForFunction(() => document.querySelector("#overlay svg .grid-track"), null, { timeout: 10000 }).catch(() => {});
  const ruled = await beside();
  check((await page.locator("#overlay svg .grid-track").count()) > 0 && bare.equals(ruled), "zoomed in, the grid draws within the slide's frame, none of it on the desk");
  await page.keyboard.press("Control+Quote");
  await page.waitForFunction(() => !document.querySelector("#overlay svg .grid-track"), null, { timeout: 10000 }).catch(() => {});

  // A click selects what is under it through the view.
  const [px, py] = await client([cx + cw / 2, cy + ch / 2]);
  await page.mouse.click(px, py);
  await page.waitForFunction(() => window.scaena.canvas.selected() === "rev", null, { timeout: 10000 }).catch(() => {});
  check((await page.evaluate(() => window.scaena.canvas.selected())) === "rev", "a click selects the chart under it, zoomed in");

  // A drag twice as far on the screen at twice as close lands where the drag at the whole canvas did.
  const atTwo = await dragged("rev", 2 * d);
  check(atTwo.every((v, i) => near(v, atFit[i])), `a drag reads the pointer through the view: ${atTwo.map(Math.round)}, ${atFit.map(Math.round)} at the whole canvas`);
  check((await source()) === original, "one undo takes it back");

  // The wheel pans.
  const before = await view();
  const [mx, my] = await client([before[0] + before[2] / 2, before[1] + before[3] / 2]);
  await page.mouse.move(mx, my);
  await page.mouse.wheel(0, 120);
  await page.waitForFunction((y) => window.scaena.canvas.view()[1] !== y, before[1], { timeout: 10000 }).catch(() => {});
  const panned = await view();
  check(panned[1] > before[1] && panned[2] === before[2], `the wheel pans down: y ${Math.round(before[1])} → ${Math.round(panned[1])}`);

  // ⌘ or Ctrl with the wheel zooms about the pointer: the point under it stays put.
  const pointer = [mx - 200, my - 100];
  const under = await canvasAt(pointer);
  await page.mouse.move(...pointer);
  await page.keyboard.down("Control");
  await page.mouse.wheel(0, -240);
  await page.keyboard.up("Control");
  await page.waitForFunction((w) => window.scaena.canvas.view()[2] !== w, panned[2], { timeout: 10000 }).catch(() => {});
  const still = await canvasAt(pointer);
  check((await zoomed()) > 2 && near(still[0], under[0]) && near(still[1], under[1]), `a pinch zooms about the pointer: ${under.map(Math.round)} stays under it at ${Math.round((await zoomed()) * 100)}%`);

  // Space and a drag pan, and select nothing, and change nothing.
  const selectedBefore = await page.evaluate(() => window.scaena.canvas.selected());
  const from = await view();
  const unchanged = await trips();
  await page.locator("#overlay").focus();
  await page.keyboard.down(" ");
  await page.mouse.move(mx, my);
  await page.mouse.down();
  for (let k = 1; k <= 6; k++) await page.mouse.move(mx - 30 * k, my - 20 * k);
  await page.mouse.up();
  await page.keyboard.up(" ");
  const to = await view();
  check(to[0] > from[0] && to[1] > from[1], `Space and a drag pan: ${from.slice(0, 2).map(Math.round)} → ${to.slice(0, 2).map(Math.round)}`);
  check((await page.evaluate(() => window.scaena.canvas.selected())) === selectedBefore && (await trips()) === unchanged, "selecting and changing nothing");

  // With nothing selected, the arrow keys pan (PLAN 2.125): about the middle at 400%, a tenth of
  // what is shown, and with Shift half.
  await page.evaluate(([w, h]) => window.scaena.canvas.zoom(4, [w / 2, h / 2]), [W, H]);
  await page.locator("#overlay").focus();
  await page.keyboard.press("Escape");
  await page.waitForFunction(() => window.scaena.canvas.selected() === undefined, null, { timeout: 10000 }).catch(() => {});
  const middle = await view();
  const keyed = async (key) => {
    const was = (await view()).join();
    await page.keyboard.press(key);
    await page.waitForFunction((v) => window.scaena.canvas.view().join() !== v, was, { timeout: 10000 }).catch(() => {});
    return view();
  };
  const right = await keyed("ArrowRight");
  check(near(right[0], middle[0] + middle[2] / 10) && right[1] === middle[1] && right[2] === middle[2], `→ pans a tenth of what is shown: x ${Math.round(middle[0])} → ${Math.round(right[0])}`);
  const down = await keyed("Shift+ArrowDown");
  check(near(down[1], right[1] + right[3] / 2) && down[0] === right[0], `Shift+↓ pans half: y ${Math.round(right[1])} → ${Math.round(down[1])}`);
  const left = await keyed("ArrowLeft");
  const up = await keyed("Shift+ArrowUp");
  check(near(left[0], middle[0]) && near(up[1], middle[1]), `← and Shift+↑ pan back: ${up.slice(0, 2).map(Math.round)}`);
  check((await page.evaluate(() => window.scaena.canvas.selected())) === undefined && (await trips()) === unchanged && (await zoomed()) === 4, "selecting and changing nothing, as close as it was");

  // Tab selects each object in reading order, and brings what it selects into view: the least
  // move that shows it whole, or, larger than what is shown, nothing but it.
  const inView = (r, v) =>
    [0, 1].every((a) => (r[a + 2] <= v[a + 2] ? r[a] >= v[a] - 0.5 && r[a] + r[a + 2] <= v[a] + v[a + 2] + 0.5 : r[a] <= v[a] + 0.5 && r[a] + r[a + 2] >= v[a] + v[a + 2] - 0.5));
  const tabbed = [];
  let moved = 0;
  for (let k = 0; k < 3; k++) {
    const was = await view();
    await page.keyboard.press("Tab");
    await page.waitForFunction(() => window.scaena.canvas.selected() !== undefined, null, { timeout: 10000 }).catch(() => {});
    await page.waitForTimeout(200);
    const node = await page.evaluate(() => window.scaena.canvas.selected());
    const [r, v] = [await box(node), await view()];
    if (v.join() !== was.join()) moved++;
    tabbed.push(`${node} ${r && inView(r, v) ? "in view" : `out of view at ${v.map(Math.round)}`}`);
  }
  check(tabbed.every((t) => t.endsWith("in view")) && moved > 0, `Tab brings each object it selects into view: ${tabbed.join(", ")}`);
  check((await trips()) === unchanged, "moving nothing");
  await page.keyboard.press("Escape");

  // Typed in where it stands, zoomed in: the caret where the text was pressed, and the input an
  // input method shows what it composes in under the caret, through the view.
  await page.evaluate(() => window.scaena.canvas.zoom("fit"));
  const [tx, ty, tw, th] = await box("title");
  await page.evaluate(([x, y]) => window.scaena.canvas.zoom(3, [x, y]), [tx + tw / 2, ty + th / 2]);
  const [qx, qy] = await client([tx + tw / 2, ty + th / 2]);
  await page.mouse.dblclick(qx, qy);
  await page.waitForFunction(() => window.scaena.canvas.typing() === "title", null, { timeout: 10000 }).catch(() => {});
  check((await page.evaluate(() => window.scaena.canvas.typing())) === "title", "a double click types in the title, zoomed in");
  await page.waitForFunction(() => document.querySelector("#overlay svg rect.caret"), null, { timeout: 10000 }).catch(() => {});
  const typed = await page.evaluate(() => {
    const [t, c] = [document.querySelector("#overlay textarea.typing"), document.querySelector("#overlay svg rect.caret")];
    const n = (a) => parseFloat(c.getAttribute(a));
    return t && c ? { left: parseFloat(t.style.left), top: parseFloat(t.style.top), caret: [n("x") + n("width") / 2, n("y") + n("height")] } : undefined;
  });
  const r = await page.locator("#overlay").boundingBox();
  const [ex, ey] = typed ? await client(typed.caret) : [NaN, NaN];
  const glyph = (th / 2) * ((await zoomed()) / unit1);
  check(typed !== undefined && near(ex, qx, glyph), `the caret where the text was pressed: ${Math.round(ex)}, pressed at ${Math.round(qx)}`);
  check(typed !== undefined && near(r.x + typed.left, ex, 1.5) && near(r.y + typed.top, ey, 1.5), `its input under the caret, through the view: ${typed && [typed.left, typed.top].map(Math.round)}`);
  // ⌘+ while typing zooms, and typing goes on.
  await page.keyboard.press("Control+Equal");
  await page.waitForFunction(() => window.scaena.canvas.zoomed() > 3.5, null, { timeout: 10000 }).catch(() => {});
  check((await zoomed()) === 4 && (await page.evaluate(() => window.scaena.canvas.typing())) === "title", `⌘+ while typing zooms, typing on: ${Math.round((await zoomed()) * 100)}%`);
  await page.keyboard.press("Escape");

  // ⌘0 shows the whole canvas again.
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+0");
  await page.waitForFunction(() => window.scaena.canvas.zoomed() === 1, null, { timeout: 10000 }).catch(() => {});
  const fit = await view();
  check(fit[0] === 0 && fit[1] === 0 && fit[2] === W && (await shows()) === "100%", `⌘0: the whole canvas, ${await shows()}`);
  // Once the worker has painted it so.
  let again = await shot();
  for (let k = 0; k < 40 && !again.equals(whole); k++) {
    await page.waitForTimeout(250);
    again = await shot();
  }
  check(again.equals(whole), "painted as it was");

  // ⌘+ and ⌘0 zoom the slide from anywhere on the page that does not take them itself, as from a
  // slide focused in the strip: not the browser's zoom of the whole page.
  await page.locator("#strip li").first().focus();
  // Heard once the page's own handlers have had it: whether one took it from the browser.
  await page.evaluate(() => addEventListener("keydown", (e) => e.key === "=" && (window.heardTaken ??= e.defaultPrevented)));
  await page.keyboard.press("Control+Equal");
  await page.waitForFunction(() => window.heardTaken !== undefined, null, { timeout: 5000 }).catch(() => {});
  check((await page.evaluate(() => window.heardTaken)) === true, "⌘+ in the strip is the editor's, not the browser's");
  await page.waitForFunction(() => window.scaena.canvas.zoomed() === 1.5, null, { timeout: 10000 }).catch(() => {});
  check((await zoomed()) === 1.5 && (await shows()) === "150%", `⌘+ in the strip zooms the slide: ${await shows()}`);
  await page.keyboard.press("Control+0");
  await page.waitForFunction(() => window.scaena.canvas.zoomed() === 1, null, { timeout: 10000 }).catch(() => {});
  check((await zoomed()) === 1 && (await shows()) === "100%", `and ⌘0 there shows the whole canvas: ${await shows()}`);

  // Another format shows all of its canvas.
  await page.evaluate(() => window.scaena.canvas.zoom(2));
  const formats = await page.evaluate(() => [...document.querySelectorAll("#format option")].map((o) => o.value).filter(Boolean));
  if (formats.length) {
    await page.locator("#format").selectOption(formats[0]);
    await page.waitForFunction(() => window.scaena.canvas.zoomed() === 1, null, { timeout: 30000 }).catch(() => {});
    check((await zoomed()) === 1 && (await shows()) === "100%", `another format shows all of its canvas: ${formats[0]}, ${await shows()}`);
  }
  check(await page.evaluate(() => window.scaena.last().valid), "and the deck still validates");
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

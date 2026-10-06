// PLAN 2.68 check: a shape's points and corners on the canvas, in the editor in headless Chromium
// (serve.mjs), the CPU painting, on the torture deck's `shapes` case: `shape-tri`, a polygon of
// three points; `shape-rule`, a line that gives none; and `shape-panel`, a rect at `radius.3`.
//
//   node web/points.mjs     (after `just web`; from the repository's root)
//
// - A polygon selected shows a handle on each of its points and one at each edge's middle; a
//   line's handles stand on the two points it draws when it gives none.
// - A point dragged moves within the box, one `choose` of `points` written where they live, and
//   one undo takes it back; with Alt, it is kept to the state shown.
// - A handle at an edge's middle adds a point there; a point clicked is picked, and Delete takes
//   it away, though never below the fewest its kind takes.
// - A rect's corner handle rounds it to the theme's radius steps: `choose` of `radius.N`.
// - axe-core finds nothing with a shape's handles shown, and the keys sheet lists them.
// Exits 1 on any failure.
import { createRequire } from "node:module";
import { launch, serve } from "./serve.mjs";

const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};

const axe = createRequire(import.meta.url).resolve("axe-core/axe.min.js");
const site = await serve();
const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1 });
  const page = await context.newPage();
  page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
  page.on("console", (m) => m.type() === "error" && failures.push(`console: ${m.text()}`));
  await page.goto(`${site.origin}/web/dist/editor.html?painter=cpu&bundle=/tests/fixtures/torture.scaena/deck.json`);
  await page.waitForFunction(() => window.scaena?.last()?.valid, null, { timeout: 120000 });
  await page.selectOption("#state", "shapes");
  await page.waitForFunction(() => window.scaena.canvas.boxed() === "shapes", null, { timeout: 30000 }).catch(() => {});

  const source = () => page.evaluate(() => window.scaena.source());
  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  /** Whether the status comes to say `text`. */
  const says = async (text) => {
    await page.waitForFunction((t) => document.querySelector("#status").textContent.includes(t), text, { timeout: 30000 }).catch(() => {});
    return (await status()).includes(text);
  };
  /** Whether the source comes to be `to`. */
  const back = async (to) => {
    await page.waitForFunction((s) => window.scaena.source() === s, to, { timeout: 30000, polling: 50 }).catch(() => {});
    return (await source()) === to;
  };
  /** Wait for the source to say `text`, or stop saying it. */
  const comes = (text, yes = true) =>
    page.waitForFunction(([t, y]) => window.scaena.source().includes(t) === y, [text, yes], { timeout: 30000 }).catch(() => {});
  const undo = async () => {
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+z");
  };
  /** The line that declares `node` in the source. */
  const declared = (text, node) => text.split("\n").find((l) => l.startsWith(`  ${node} `)) ?? "";
  /** A point in canvas units, on the page. */
  const onPage = ([x, y]) =>
    page.evaluate(
      ([x, y]) => {
        const r = document.querySelector("#overlay").getBoundingClientRect();
        const [w, h] = window.scaena.canvas.size();
        return [r.left + (x / w) * r.width, r.top + (y / h) * r.height];
      },
      [x, y],
    );
  const click = async (at) => {
    const [x, y] = await onPage(at);
    await page.mouse.click(x, y);
  };
  const box = (node) => page.evaluate((n) => window.scaena.canvas.boxes()?.find((b) => b.node === n), node);
  const selects = async (node) => {
    await page.waitForFunction((n) => window.scaena.canvas.selected() === n, node, { timeout: 30000 }).catch(() => {});
    return (await page.evaluate(() => window.scaena.canvas.selected())) === node;
  };
  /** The outline the canvas holds for the shape selected, once it holds `node`'s with `points`
   * (as many, or those). */
  const outline = async (node, points) => {
    await page
      .waitForFunction(([n, p]) => {
        const o = window.scaena.canvas.outlined();
        if (o?.node !== n) return false;
        return p === undefined || (typeof p === "number" ? o.points.length === p : JSON.stringify(o.points) === JSON.stringify(p));
      }, [node, points], { timeout: 30000 })
      .catch(() => {});
    return page.evaluate(() => window.scaena.canvas.outlined());
  };
  const TRI = [[0.5, 0], [1, 1], [0, 1]];
  const RULE = [[0, 0.5], [1, 0.5]];
  /** The middle of handle `selector`, canvas units, once it is drawn. */
  const handle = async (selector) => {
    await page.waitForSelector(`#overlay svg ${selector}`, { timeout: 30000 }).catch(() => {});
    return page.evaluate((s) => {
      const c = document.querySelector(`#overlay svg ${s}`);
      return c ? [Number(c.getAttribute("cx")), Number(c.getAttribute("cy"))] : undefined;
    }, selector);
  };
  /** Press at `from` and go to `to` (canvas units) a step at a time, `keys` held, and let go. */
  const drag = async (from, to, keys = []) => {
    const [fx, fy] = await onPage(from);
    await page.mouse.move(fx, fy);
    for (const key of keys) await page.keyboard.down(key);
    await page.mouse.down();
    const steps = 8;
    for (let k = 1; k <= steps; k++) {
      const [x, y] = await onPage([from[0] + ((to[0] - from[0]) * k) / steps, from[1] + ((to[1] - from[1]) * k) / steps]);
      await page.mouse.move(x, y);
    }
    await page.mouse.up();
    for (const key of keys) await page.keyboard.up(key);
  };

  const original = await source();
  check(declared(original, "shape-tri").includes("points:[[0.5, 0], [1, 1], [0, 1]]"), `the triangle as written: ${declared(original, "shape-tri")}`);

  // The triangle: a handle on each point, and one at each edge's middle.
  const [tx, ty, tw, th] = (await box("shape-tri")).rect;
  const inTri = [tx + tw / 2, ty + th * 0.75];
  await click(inTri);
  check(await selects("shape-tri"), "a click in the triangle selects it");
  const o = await outline("shape-tri");
  check(o?.kind === "polygon" && o.points.length === 3 && o.fewest === 3, `its outline: ${JSON.stringify(o)}`);
  await handle('[data-point="2"]');
  check((await page.locator("#overlay svg [data-point]").count()) === 3, "a handle on each of its three points");
  check((await page.locator("#overlay svg [data-add]").count()) === 3, "and one at each edge's middle");
  const apex = await handle('[data-point="0"]');
  check(apex && Math.abs(apex[0] - (tx + tw / 2)) < 0.5 && Math.abs(apex[1] - ty) < 0.5, `the apex's handle stands on it: ${apex}`);

  // axe-core, the handles shown.
  await page.addScriptTag({ path: axe });
  const found = await page.evaluate(async () => {
    const result = await window.axe.run(document, { runOnly: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"] });
    return result.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
  });
  check(!found.length, `axe finds nothing with a shape's handles shown${found.length ? `:\n    ${found.join("\n    ")}` : ""}`);

  // Its apex dragged a quarter of the box left: one `choose` of its points, on the triangle.
  await drag(apex, [apex[0] - tw / 4, apex[1]]);
  check(await says("shape-tri's point 1 moved"), `the status says the point moved: ${await status()}`);
  await comes("points:[[0.25, 0], [1, 1], [0, 1]]");
  check(declared(await source(), "shape-tri").includes("points:[[0.25, 0], [1, 1], [0, 1]]"), `written on the triangle: ${declared(await source(), "shape-tri")}`);
  await undo();
  check(await back(original), "one undo takes it back");

  // A point dragged past its box stays at its edge: above it, the apex goes nowhere.
  check(JSON.stringify((await outline("shape-tri", TRI)).points) === JSON.stringify(TRI), "the canvas holds the triangle as it was");
  const apex2 = await handle('[data-point="0"]');
  await drag(apex2, [apex2[0], apex2[1] - th]);
  check(await says("shape-tri's point 1 stays where it is"), `a point dragged above its box stays at its edge: ${await status()}`);
  check((await source()) === original, "and nothing changes");

  // A point added at an edge's middle, then picked and taken away.
  const mid = await handle('[data-add="0"]');
  await click(mid);
  check(await says("shape-tri has a point added"), `a click on an edge's middle adds a point there: ${await status()}`);
  await comes("points:[[0.5, 0], [0.75, 0.5], [1, 1], [0, 1]]");
  check(declared(await source(), "shape-tri").includes("points:[[0.5, 0], [0.75, 0.5], [1, 1], [0, 1]]"), `between its neighbors: ${declared(await source(), "shape-tri")}`);
  await outline("shape-tri", 4);
  const added = await handle('[data-point="1"]');
  await click(added);
  await page.waitForFunction(() => window.scaena.canvas.pointPicked() === 1, null, { timeout: 10000 }).catch(() => {});
  check((await page.evaluate(() => window.scaena.canvas.pointPicked())) === 1, "a click on it picks it");
  check((await page.locator('#overlay svg [data-point="1"].picked').count()) === 1, "and draws it picked");
  await page.keyboard.press("Delete");
  check(await says("shape-tri's point 2 taken away"), `Delete takes it away: ${await status()}`);
  await comes("points:[[0.5, 0], [1, 1], [0, 1]]");
  check(declared(await source(), "shape-tri").includes("points:[[0.5, 0], [1, 1], [0, 1]]"), "the triangle as it was");
  check(await selects("shape-tri"), "and the triangle stays selected");
  // A triangle keeps three.
  await outline("shape-tri", TRI);
  const third = await handle('[data-point="2"]');
  await click(third);
  await page.waitForFunction(() => window.scaena.canvas.pointPicked() === 2, null, { timeout: 10000 }).catch(() => {});
  const before = await source();
  await page.keyboard.press("Delete");
  check(await says("a polygon keeps three points"), `a triangle keeps its three: ${await status()}`);
  check((await source()) === before, "and nothing changes");
  await page.keyboard.press("Escape");
  check((await page.evaluate(() => [window.scaena.canvas.pointPicked(), window.scaena.canvas.selected()])).join() === ",shape-tri", "Escape lets go of the point, and the triangle stays selected");
  await undo();
  await undo();
  check(await back(original), "undo twice: the deck as it was");

  // The rule gives no points: its handles stand on the two it draws, and a drag writes them.
  const [rx, ry, rw, rh] = (await box("shape-rule")).rect;
  await click([rx + rw / 2, ry + rh / 2]);
  check(await selects("shape-rule"), "a click on the rule selects it");
  const ro = await outline("shape-rule");
  check(JSON.stringify(ro?.points) === "[[0,0.5],[1,0.5]]" && ro.fewest === 2, `its two points, across its box's middle: ${JSON.stringify(ro?.points)}`);
  check((await page.locator("#overlay svg [data-add]").count()) === 1, "one edge, one middle");
  const end = await handle('[data-point="1"]');
  await drag(end, [end[0] - rw / 2, end[1]]);
  await comes("points:[[0, 0.5], [0.5, 0.5]]");
  check(declared(await source(), "shape-rule").includes("points:[[0, 0.5], [0.5, 0.5]]"), `written on the rule: ${declared(await source(), "shape-rule")}`);
  await undo();
  check(await back(original), "one undo takes it back");

  // With Alt, kept to the state shown: the state's own props say it, and the rule's line does not.
  await outline("shape-rule", RULE);
  const end2 = await handle('[data-point="1"]');
  await drag(end2, [end2[0] - rw / 2, end2[1]], ["Alt"]);
  check(await says("kept to shapes"), `the status says it is kept to the state: ${await status()}`);
  await comes("[[0, 0.5], [0.5, 0.5]]");
  const kept = await source();
  // The state's own line names the rule and its points, no type: the rule's declaration, now a
  // `node` of its own, gives none.
  const keptLines = kept.split("\n").filter((l) => l.includes("[[0, 0.5], [0.5, 0.5]]"));
  const declaration = kept.split("\n").find((l) => /shape-rule shape /.test(l)) ?? "";
  check(keptLines.length === 1 && keptLines[0].trim() === "shape-rule points:[[0, 0.5], [0.5, 0.5]]", `with Alt, in the state's own props: ${keptLines.join(" | ")}`);
  check(declaration !== "" && !declaration.includes("points:"), `and not on the rule: ${declaration}`);
  await undo();
  check(await back(original), "one undo takes it back");

  // A line keeps two points.
  await outline("shape-rule", RULE);
  await click(await handle('[data-point="0"]'));
  await page.waitForFunction(() => window.scaena.canvas.pointPicked() === 0, null, { timeout: 10000 }).catch(() => {});
  await page.keyboard.press("Delete");
  check(await says("a line keeps two points"), `a line keeps its two: ${await status()}`);

  // The panel's corner: radius.3 rounded to radius.4.
  const [px, py, pw, ph] = (await box("shape-panel")).rect;
  await click([px + pw / 2, py + ph / 2]);
  check(await selects("shape-panel"), "a click on the panel selects it");
  const po = await outline("shape-panel");
  check(po?.kind === "rect" && po.radii.length === 6 && Math.abs(po.radius - po.radii[3]) < 0.01, `its radius is the theme's step 3: ${JSON.stringify(po)}`);
  check((await page.locator("#overlay svg [data-point]").count()) === 0, "a rect has no points to drag");
  const corner = await handle("[data-radius]");
  const by = po.radii[4] - po.radius;
  await drag(corner, [corner[0] + by, corner[1] + by]);
  check(await says("shape-panel's corners round to radius.4"), `the status says the step: ${await status()}`);
  await comes("radius:radius.4");
  check(declared(await source(), "shape-panel").includes("radius:radius.4"), `written on the panel: ${declared(await source(), "shape-panel")}`);
  await undo();
  check(await back(original), "one undo takes it back");
  // Dragged back past its corner, it goes square: radius.0.
  await outline("shape-panel");
  const corner2 = await handle("[data-radius]");
  await drag(corner2, [corner2[0] - po.radius - 20, corner2[1] - po.radius - 20]);
  await comes("radius:radius.0");
  check(declared(await source(), "shape-panel").includes("radius:radius.0"), `dragged past its corner, square: ${declared(await source(), "shape-panel")}`);
  await undo();
  check(await back(original), "one undo takes it back");

  // Escape during a drag leaves the shape as it was.
  await outline("shape-panel");
  const corner3 = await handle("[data-radius]");
  const [cx, cy] = await onPage(corner3);
  await page.mouse.move(cx, cy);
  await page.mouse.down();
  await page.mouse.move(cx + 30, cy + 30, { steps: 4 });
  await page.keyboard.press("Escape");
  await page.mouse.up();
  check(await says("shape-panel stays as it is"), `Escape leaves it: ${await status()}`);
  await page.waitForTimeout(300);
  check((await source()) === original, "and nothing changes");

  // The keys sheet lists what edits a shape's outline.
  await page.locator("#overlay").focus();
  await page.keyboard.press("Escape");
  await page.keyboard.press("?");
  await page.waitForSelector("#keys[open]", { timeout: 10000 }).catch(() => {});
  const listed = await page.evaluate(() => [...document.querySelectorAll("#keys kbd")].map((k) => k.textContent));
  for (const k of ["Drag a point of the shape selected", "Click an edge's middle", "Drag a rect's corner", "Click a point, then Delete"]) {
    check(listed.some((l) => l.includes(k)), `the keys sheet lists ${k}`);
  }
} finally {
  await browser.close();
  await site.close();
}

if (failures.length) {
  console.log(`\n${failures.length} failed:\n  ${failures.join("\n  ")}`);
  process.exit(1);
}
console.log("\nall passed");

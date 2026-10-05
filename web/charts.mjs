// PLAN 2.41 check: a chart from the editor's Insert menu, and what it reads chosen in the inspector,
// in headless Chromium (serve.mjs), the CPU painting, as in web/insert.mjs.
//
//   node web/charts.mjs     (after `just web`; from the repository's root)
//
// On the revenue example, in `revenue`:
// - The Insert menu offers a chart and a table of `q3`, its one data source.
// - The chart inserted where the canvas was pressed reads the quarters, grouped by product: one
//   patch, the chart selected. The inspector offers what it reads: its source, each channel's field
//   from the columns of `q3` the channel can read (a y the numbers), and the type it reads it as.
// - Customers chosen for its y is one `choose`, and the preview draws it otherwise.
// - A second source written into the source, of segments, is offered as its source; chosen, the
//   chart reads the segments and their sales, and its series, which that source has no column for,
//   goes: one patch.
// - Each is a step to undo, and the source is as it was.
// Exits 1 on any failure.
import { createHash } from "node:crypto";
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
  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  const selected = () => page.evaluate(() => window.scaena.canvas.selected());
  /** Once the source holds `text`, and the canvas has caught up. */
  const reads = async (text) => {
    await page
      .waitForFunction((t) => window.scaena.source().includes(t) && !window.scaena.canvas.busy(), text, { timeout: 30000, polling: 50 })
      .catch(() => {});
    await page.waitForTimeout(300);
    return (await source()).includes(text);
  };
  /** The preview as painted, with nothing focused over it. */
  const painted = async () => {
    await page.waitForFunction(() => !window.scaena.canvas.busy(), null, { timeout: 30000 });
    await page.waitForTimeout(400);
    return createHash("sha256").update(await page.locator("#stage").screenshot({ animations: "disabled" })).digest("hex");
  };
  /** The words a field of the inspector offers, once it offers `has`. */
  const offers = async (prop, has) => {
    const id = `#look-${prop.replace(/\//g, "-")}`;
    await page
      .waitForFunction(([id, has]) => [...(document.querySelector(id)?.options ?? [])].some((o) => o.value === has), [id, has], { timeout: 30000 })
      .catch(() => {});
    return page.evaluate((id) => [...(document.querySelector(id)?.options ?? [])].map((o) => o.value).filter(Boolean), id);
  };
  /** Show the state declared as `state` in the source, slot `index`. */
  const showState = async (state, index) => {
    const at = (await source()).indexOf(`state ${state}`) + "state ".length;
    await page.evaluate((offset) => window.scaena.cursor(offset), at);
    await page.waitForFunction((i) => window.scaena.shown() === i && window.scaena.at().index === i, index, { timeout: 30000 });
    await page.waitForFunction((s) => window.scaena.canvas.boxed() === s && window.scaena.canvas.boxes().length > 0, state, { timeout: 30000 });
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

  // A chart and a table of `q3`.
  await page.waitForFunction(() => document.querySelectorAll("#insert optgroup").length > 0, null, { timeout: 30000 });
  const offered = await menu();
  check(offered.Chart?.includes("q3") && offered.Table?.includes("q3"), `a chart and a table of q3: ${JSON.stringify([offered.Chart, offered.Table])}`);

  // The chart, about where the canvas was pressed: the quarters, grouped by product.
  await press([480, 300]);
  const chart = await page.evaluate(() => [...document.querySelectorAll('#insert optgroup[label="Chart"] option')].find((o) => o.textContent === "q3")?.value);
  await page.selectOption("#insert", chart);
  check(await reads("q3-chart chart:bar data:@q3 x:{field: quarter} y:{field: revenue}"), "the chart of q3 is inserted");
  check((await source()).includes("series:{field: product}"), "its quarters grouped by product");
  check((await selected()) === "q3-chart", `and selected: ${await selected()}`);

  // What it reads: its source, the columns each channel can read, and the type.
  check(JSON.stringify(await offers("data", "@q3")) === JSON.stringify(["@q3"]), "its source, from the deck's");
  check(
    JSON.stringify(await offers("x/field", "quarter")) === JSON.stringify(["quarter", "product", "revenue", "customers"]),
    `its x, any column: ${await offers("x/field", "quarter")}`,
  );
  check(JSON.stringify(await offers("y/field", "revenue")) === JSON.stringify(["revenue", "customers"]), `its y, the numbers: ${await offers("y/field", "revenue")}`);
  check((await offers("x/type", "ordinal")).includes("temporal"), "the type it reads x as");

  // Customers, not revenue.
  const before = await painted();
  await page.selectOption("#look-y-field", "customers");
  check(await reads("y:{field: customers}"), "customers chosen for its y is the chart's");
  check((await painted()) !== before, "and the preview draws it otherwise");

  // A second source, of segments, written into the source, then chosen as the chart's.
  // After the whole of q3's declaration: its line, and the lines under it that go on with it.
  const lines = (await source()).split("\n");
  let after = lines.findIndex((l) => l.startsWith("data q3")) + 1;
  while (lines[after]?.startsWith("  ")) after++;
  lines.splice(
    after,
    0,
    "data segments",
    "  inline:[{segment: Core, sales: 30.1}, {segment: Pro, sales: 21.4}, {segment: Enterprise, sales: 15.2}]",
    "  schema:{sales: number}",
  );
  await page.evaluate((text) => window.scaena.type(text), lines.join("\n"));
  check(JSON.stringify(await offers("data", "@segments")) === JSON.stringify(["@q3", "@segments"]), "the new source is offered");
  await page.selectOption("#look-data", "@segments");
  check(await reads("q3-chart chart:bar data:@segments x:{field: segment} y:{field: sales}"), "the chart reads the segments and their sales");
  check(!(await source()).includes("series:{field: product}"), "its series, which segments have no column for, goes");
  check((await status()).includes("q3-chart"), `the status says what changed: ${await status()}`);

  // Each a step to undo.
  await page.locator("#overlay").focus();
  for (let i = 0; i < 4 && (await source()) !== original; i++) {
    await page.keyboard.press("Control+z");
    await page.waitForTimeout(500);
  }
  const back = await page
    .waitForFunction((s) => window.scaena.source() === s, original, { timeout: 30000 })
    .then(() => true)
    .catch(() => false);
  check(back, "four undos make the source what it was");
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

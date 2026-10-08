// PLAN 2.96 check: pasting on the editor's canvas what another app copied, in headless Chromium
// (serve.mjs), the CPU painting, on the revenue example's closing slide.
//
//   node web/paste.mjs     (after `just web`; from the repository's root)
//
// - A sheet's cells, as Google Sheets and Excel copy them (rows of tab-separated cells, with an
//   HTML table and a picture of them beside), become a data source and the table they were, its
//   figures printed as they were copied: `1,200`, `$1,234.50`, `12.5%`. Two steps to undo: the
//   table, then the source.
// - A screenshot (a PNG with no words) comes in as an image where the canvas was last pressed.
// - A WebP comes in as a PNG of it, and the status says so.
// - A file copied where files are kept, whose words only name it, comes in as the picture.
// - Words copied with a picture of them (Keynote's text) come in as a text, not the picture.
// - A file neither a picture nor data changes nothing, and the status says why.
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
  await page.waitForFunction(() => window.scaena?.last()?.valid, null, { timeout: 120000 });

  const source = () => page.evaluate(() => window.scaena.source());
  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  const selected = () => page.evaluate(() => window.scaena.canvas.selected());
  const says = async (text) => {
    await page.waitForFunction((t) => document.querySelector("#status").textContent.includes(t), text, { timeout: 30000 }).catch(() => {});
    return (await status()).includes(text);
  };
  /** Once the source differs from `was`, and the canvas has caught up. */
  const changed = async (was) => {
    await page.waitForFunction((s) => window.scaena.source() !== s && !window.scaena.canvas.busy(), was, { timeout: 30000, polling: 50 }).catch(() => {});
    await page.waitForTimeout(300);
    return (await source()) !== was;
  };
  const back = async (to) => {
    await page.waitForFunction((s) => window.scaena.source() === s, to, { timeout: 30000, polling: 50 }).catch(() => {});
    await page.waitForTimeout(300);
    return (await source()) === to;
  };
  const undo = async (to) => {
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+z");
    return back(to);
  };
  /** Press the canvas at `[x, y]`, canvas units. */
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
    await page.waitForFunction(() => !window.scaena.canvas.busy(), null, { timeout: 30000 });
  };
  /** A paste on the canvas, focused, as ⌘V fires it: `data` by media type, and `files`, each
   * `{ name, type, url }` fetched from the site or `{ name, type, webp }` drawn as a WebP. */
  const paste = (data, files = []) =>
    page.evaluate(
      async ([data, files]) => {
        const held = new DataTransfer();
        for (const [type, value] of Object.entries(data)) held.setData(type, value);
        for (const f of files) {
          let bytes;
          if (f.url) bytes = await (await fetch(f.url)).arrayBuffer();
          else {
            const canvas = new OffscreenCanvas(64, 48);
            const g = canvas.getContext("2d");
            g.fillStyle = "#c0392b";
            g.fillRect(0, 0, 64, 48);
            g.fillStyle = "#f1c40f";
            g.fillRect(16, 12, 32, 24);
            bytes = await (await canvas.convertToBlob({ type: "image/webp" })).arrayBuffer();
          }
          held.items.add(new File([bytes], f.name, { type: f.type }));
        }
        const e = new ClipboardEvent("paste", { clipboardData: held, bubbles: true, cancelable: true });
        document.querySelector("#overlay").focus();
        document.querySelector("#overlay").dispatchEvent(e);
        return e.defaultPrevented;
      },
      [data, files],
    );

  // The closing slide, with room below its title.
  const index = await page.evaluate(() => window.scaena.opened.states.indexOf("close"));
  await page.evaluate((offset) => window.scaena.cursor(offset), (await source()).indexOf("state close") + "state ".length);
  await page.waitForFunction((i) => window.scaena.shown() === i && window.scaena.canvas.boxes().length > 0, index, { timeout: 30000 });
  const original = await source();
  /** How many of the bundle's pictures a source names. */
  const kept = (text) => text.split("assets/").length - 1;
  const ridge = { name: "image.png", type: "image/png", url: "/docs/examples/assets/trails-ridge.png" };

  // A sheet's cells, as Excel copies them: with an HTML table and a picture of them beside.
  await press([960, 760]);
  const sheet = "Trail\tVisitors\tRevenue\tShare\r\nRidge\t1,200\t$1,234.50\t12.5%\r\nCreek\t950\t$987.00\t8%\r\n";
  const html = "<table><tr><td>Trail</td><td>Visitors</td></tr></table>";
  check(await paste({ "text/plain": sheet, "text/html": html }, [ridge]), "the canvas takes the cells");
  check(await changed(original), "the cells declare a source");
  const declared = await source();
  check(declared.includes("trail-visitors-revenue"), "named for its first columns: @trail-visitors-revenue");
  await page.waitForFunction(() => /table/.test(window.scaena.canvas.selected() ?? ""), null, { timeout: 30000 }).catch(() => {});
  const table = await selected();
  check(/table/.test(table ?? ""), `the table they were is inserted, selected: ${table}`);
  check(await says("2 rows pasted attached as @trail-visitors-revenue") || (await says("inserted as")), `the status says so: ${await status()}`);
  const scn = await source();
  for (const format of ['",d"', '"$,.2f"', '".1~%"']) check(scn.includes(format), `its columns print as copied: ${format}`);
  check(kept(scn) === kept(original), "the picture beside the cells stays out");
  const read = await page.evaluate(() => window.scaena.reading());
  for (const figure of ["1,200", "$1,234.50", "12.5%", "8%"]) check(read.includes(figure), `the table reads ${figure}, as copied`);
  check(await undo(declared), "⌘Z takes the table away");
  check(await undo(original), "and again the source: the deck is as it was");

  // A screenshot: a PNG and no words, where the canvas was last pressed.
  await press([960, 760]);
  check(await paste({}, [ridge]), "the canvas takes a screenshot");
  check(await changed(original), "the screenshot comes in");
  check(await says("image.png inserted as image"), `as an image named for it: ${await status()}`);
  check((await selected()) === "image", `selected: ${await selected()}`);
  check(kept(await source()) === kept(original) + 1, "its file joins the bundle, and the image shows it");
  check(await undo(original), "⌘Z takes it away");

  // A WebP: a PNG of it.
  check(await paste({}, [{ name: "swatch.webp", type: "image/webp" }]), "the canvas takes a WebP");
  check(await changed(original), "the WebP comes in");
  check(await says("kept as a PNG"), `as a PNG of it: ${await status()}`);
  check((await selected()) === "swatch", `named for its file: ${await selected()}`);
  check(await undo(original), "⌘Z takes it away");

  // A file copied where files are kept: its words only name it.
  check(await paste({ "text/plain": "trails-ridge.png" }, [{ ...ridge, name: "trails-ridge.png" }]), "the canvas takes a file copied");
  check(await changed(original), "the file comes in");
  check((await selected()) === "trails-ridge", `as the picture, named for it: ${await selected()}`);
  check(await undo(original), "⌘Z takes it away");

  // Words with a picture of them: the words.
  check(await paste({ "text/plain": "Margins held." }, [ridge]), "the canvas takes words with a picture");
  check(await changed(original), "the words come in");
  const worded = await source();
  check(worded.includes("Margins held.") && kept(worded) === kept(original), "as a text, not the picture");
  check(await undo(original), "⌘Z takes them away");

  // Neither a picture nor data.
  const pdf = { name: "brief.pdf", type: "application/pdf", url: "/docs/examples/assets/SOURCES.md" };
  await paste({}, [pdf]);
  check(await says("brief.pdf is neither a picture (PNG, JPEG) nor data (CSV, JSON)"), `a file neither changes nothing: ${await status()}`);
  check((await source()) === original, "the deck is as it was");
} finally {
  await browser.close();
  await server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "what another app copied pastes as a picture, a table of its cells, or its words");
process.exit(failures.length ? 1 : 0);

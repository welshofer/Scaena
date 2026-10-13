// PLAN 2.104 check: an icon inserted from the editor's picker, in headless Chromium (serve.mjs),
// the CPU painting. On the revenue example in Dusk:
//
// - The Insert menu offers Icon…, which opens the picker: every icon that ships, drawn, a few
//   hundred at a time, with how many there are.
// - Words find icons as `scaena icons` finds them: `rocket` first for "rocket", none for nonsense.
// - A click on one inserts it where the slide was last pressed: one `add_node` of a path shape on
//   the icon's view, in the theme's accent, as decoration named by the icon, selected, and drawn
//   there and nowhere else; the status says so.
// - Solid draws the fill weight, another path on the same view; Return in the field inserts the
//   first found, a second icon named apart from the first.
// - Escape leaves the picker with nothing inserted (the first clears its words, as a search field's
//   Escape does), and ⌘Z takes each insert back.
//
//   node web/icons.mjs     (after `just web`; from the repository's root)
//
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
  const context = await browser.newContext({ viewport: { width: 1920, height: 1200 }, deviceScaleFactor: 1, acceptDownloads: true });
  const page = await context.newPage();
  page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
  page.on("console", (m) => m.type() === "error" && failures.push(`console: ${m.text()}`));
  await page.goto(`${server.origin}/web/dist/editor.html?painter=cpu&bundle=/docs/examples/revenue.deck.json`);
  await page.waitForFunction(() => window.scaena?.last()?.valid && window.scaena.canvas.boxes()?.length, null, { timeout: 120000 });

  const source = () => page.evaluate(() => window.scaena.source());
  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  /** Once the source satisfies `test`, and the canvas has caught up. */
  const until = async (test, arg) => {
    await page.waitForFunction(test, arg, { timeout: 30000, polling: 50 }).catch(() => {});
    await page.waitForFunction(() => !window.scaena.canvas.busy(), null, { timeout: 30000 }).catch(() => {});
    await page.waitForTimeout(300);
  };
  const undo = async () => {
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+z");
  };
  /** The slide shown as the PNG export draws it, 960 wide, as RGBA. */
  const drawn = () =>
    page.evaluate(async () => {
      const shot = await window.scaena.exportAs("png", 960);
      const bitmap = await createImageBitmap(new Blob([shot.bytes], { type: "image/png" }));
      const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
      const g = canvas.getContext("2d");
      g.drawImage(bitmap, 0, 0);
      const { data, width, height } = g.getImageData(0, 0, bitmap.width, bitmap.height);
      return { data: Array.from(data), width, height };
    });
  /** The box, in the export's pixels, of every pixel that differs between two drawings. */
  const changed = (a, b) => {
    let [x0, y0, x1, y1, n] = [Infinity, Infinity, -1, -1, 0];
    for (let y = 0; y < a.height; y++) {
      for (let x = 0; x < a.width; x++) {
        const i = (y * a.width + x) * 4;
        const d = Math.abs(a.data[i] - b.data[i]) + Math.abs(a.data[i + 1] - b.data[i + 1]) + Math.abs(a.data[i + 2] - b.data[i + 2]);
        if (d < 24) continue;
        n++;
        [x0, y0, x1, y1] = [Math.min(x0, x), Math.min(y0, y), Math.max(x1, x), Math.max(y1, y)];
      }
    }
    return { n, box: [x0, y0, x1, y1] };
  };
  /** Open the picker from the Insert menu, and wait for its icons. */
  const open = async () => {
    await page.selectOption("#insert", "icon");
    await page.waitForSelector("#iconing[open] .icons button.icon", { timeout: 30000 });
  };
  const picker = () =>
    page.evaluate(() => ({
      shown: [...document.querySelectorAll("#iconing .icons button.icon")].map((b) => b.dataset.icon),
      count: document.querySelector("#iconing .count").textContent,
      open: document.querySelector("#iconing").open,
    }));
  /** The source's lines for node `id`, joined: its line and those under it, indented deeper. */
  const line = async (id) => {
    const lines = (await source()).split("\n");
    const at = lines.findIndex((l) => l.trimStart().startsWith(`${id} shape `));
    if (at < 0) return "";
    const indent = (l) => l.length - l.trimStart().length;
    const end = lines.findIndex((l, i) => i > at && indent(l) <= indent(lines[at]));
    return lines.slice(at, end < 0 ? undefined : end).map((l) => l.trim()).join(" ");
  };
  /** Press the canvas at `at`, canvas units: where what is inserted lands. */
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
  await page.evaluate(() => window.scaena.cursor(window.scaena.source().indexOf("state revenue") + "state ".length));
  await page.waitForFunction(() => window.scaena.shown() === 1 && window.scaena.canvas.boxed() === "revenue", null, { timeout: 30000 });

  const original = await source();
  const offered = await page.evaluate(() => [...document.querySelectorAll("#insert option")].map((o) => o.textContent));
  check(offered.includes("Icon…"), `the Insert menu offers Icon…: ${offered.slice(-3)}`);

  // The picker: every icon, drawn, a few hundred at a time.
  // The point the slide was last pressed, where an icon lands: right of the title, clear of the rest.
  await press([1500, 200]);
  await page.evaluate(() => window.scaena.canvas.select(undefined));
  const bare = await drawn();
  await open();
  let seen = await picker();
  check(seen.shown.length === 240 && /^240 of 14\d\d icons/.test(seen.count), `the picker draws the first icons and counts them all: ${seen.shown.length}, ${seen.count}`);

  // Words find icons as `scaena icons` does.
  await page.fill("#iconing input.find", "zzqx");
  await page.waitForFunction(() => document.querySelector("#iconing .count").textContent.startsWith("No icon"), null, { timeout: 10000 });
  check((await picker()).shown.length === 0, "nonsense finds none, and says so");
  await page.fill("#iconing input.find", "rocket");
  await page.waitForFunction(() => document.querySelector("#iconing .icons button.icon")?.dataset.icon === "rocket", null, { timeout: 10000 });
  seen = await picker();
  check(seen.shown[0] === "rocket" && seen.shown.every((n) => n.includes("rocket")), `"rocket" finds the rockets, its name first: ${seen.shown}`);

  // A click inserts it where the slide was last pressed: one path shape on its view.
  const regular = await page.evaluate(() => document.querySelector('#iconing button.icon[data-icon="rocket"] path').getAttribute("d"));
  await page.click('#iconing button.icon[data-icon="rocket"]');
  await until(() => /^\s*rocket shape /m.test(window.scaena.source()));
  check(!(await picker()).open, "choosing an icon closes the picker");
  const rocket = await line("rocket");
  check(rocket.includes("kind:path") && rocket.includes(`path:"${regular}"`), `inserted as a path shape, the icon's own: ${rocket.slice(0, 60)}…`);
  check(rocket.includes("view:[0, 0, 256, 256]") && rocket.includes("fill:accent"), `on its view, in the theme's accent: ${rocket.replace(/path:"[^"]*"/, "path:…")}`);
  check(rocket.includes("semantic:decoration") && /\balt:\"?Rocket\b/.test(rocket), "decoration named by the icon");
  check(await page.evaluate(() => window.scaena.canvas.selected()?.includes("rocket")), "the icon is selected");
  check((await status()).includes("The icon rocket inserted on this slide"), `the status says so: ${await status()}`);
  const once = changed(bare, await drawn());
  const [x0, y0, x1, y1] = once.box;
  check(once.n > 100 && x0 >= 960 * (1300 / 1920) && x1 <= 960 * (1700 / 1920) && y1 <= 540 * (400 / 1080), `drawn about the point pressed, nowhere else: ${once.n} px in ${once.box}`);

  // Solid draws the fill weight; Return inserts the first found, named apart.
  await open();
  await page.check("#iconing input.solid");
  await page.fill("#iconing input.find", "rocket");
  await page.waitForFunction(() => document.querySelector("#iconing .icons button.icon")?.dataset.icon === "rocket", null, { timeout: 10000 });
  const solid = await page.evaluate(() => document.querySelector('#iconing button.icon[data-icon="rocket"] path').getAttribute("d"));
  check(solid !== regular, "Solid draws the fill weight, another path");
  await page.press("#iconing input.find", "Enter");
  await until(() => /^\s*rocket-2 shape /m.test(window.scaena.source()));
  check((await line("rocket-2")).includes(`path:"${solid}"`), "Return inserts the first found, solid, as rocket-2");

  // Escape inserts nothing: the first clears the words, as a search field's does, the next leaves.
  const before = await source();
  await open();
  await page.keyboard.press("Escape");
  if (await page.evaluate(() => document.querySelector("#iconing").open)) await page.keyboard.press("Escape");
  await page.waitForFunction(() => !document.querySelector("#iconing").open, null, { timeout: 10000 });
  await page.waitForTimeout(300);
  check((await source()) === before, "Escape leaves the picker with nothing inserted");

  // Each insert is one step to undo.
  await undo();
  await undo();
  await until((s) => window.scaena.source() === s, original);
  check((await source()) === original, "two undos take both icons back");
} finally {
  await browser.close();
  await server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "an icon is found and inserted from the picker, each a patch");
process.exit(failures.length ? 1 : 0);

// A deck edited, as a person edits one (PLAN 2.77): the static site's demo deck (trails) in the
// editor, in headless Chromium, driven by its buttons, menus, keys, and drops alone. The CPU
// paints. `window.scaena` is only read, to wait and to check; nothing is done through it. The
// counterpart of web/first-deck.mjs, which makes a deck from nothing.
//
//   node web/edit-deck.mjs     (after `just site`; from the repository's root)
//
// - The theme picker re-themes the deck in Daybreak, where each contrast error the storm slide's
//   photo makes is fixed in one click and undone, and back in the theme it had.
// - In the strip: a slide moved with Alt+→, one renamed with F2, one deleted with Delete, and the
//   delete undone with ⌘Z.
// - A text clicked on the canvas takes another role and another color from the inspector.
// - A cell set in the Data tab: the chart that reads it is drawn again, changed.
// - A figure of the Data tab quoted into a text's characters (PLAN 2.72).
// - ⌘F finds a word across the deck, and Replace All replaces every match.
// - The deck in 9:16 from the format picker: the process slide's steps one above another, and
//   nothing for lint there (PLAN 2.84). A node moved there moves there alone, and the right-click
//   menu places another anew there (PLAN 2.85). ⌘D there puts a copy clear of its node in 9:16
//   and on the deck's own canvas (PLAN 2.86).
// - Rehearse plays two slides, and Keep makes their times holds.
// - Play, then the presenter view, which goes on and the player with it.
// - Export… gives a PDF, and Download the .scaena.
//
// Each step is screenshotted into target/edit-deck/, with its time. Exits 1 on any failure.
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { decode, launch, serve } from "./serve.mjs";

const out = "target/edit-deck";
await mkdir(out, { recursive: true });
const root = join(process.cwd(), "target/site");
const media = { ".css": "text/css", ".png": "image/png", ".ttf": "font/ttf", ".csv": "text/csv", ".txt": "text/plain", ".svg": "image/svg+xml", ".webmanifest": "application/manifest+json" };
const server = await serve(root, { more: media });
const editor = `${server.origin}/editor.html?painter=cpu`;
const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
  return ok;
};

const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1, acceptDownloads: true });
  const page = await context.newPage();
  page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
  page.on("console", (m) => m.type() === "error" && failures.push(`console: ${m.text()}`));
  page.on("dialog", (d) => d.accept());

  let shot = 0;
  const snap = (name, on = page) => on.screenshot({ path: join(out, `${String(++shot).padStart(2, "0")}-${name}.png`) });
  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  const source = () => page.evaluate(() => window.scaena.source());
  /** Where the editor is: its source, and how many edits it has compiled and linted. */
  const mark = () => page.evaluate(() => ({ source: window.scaena.source(), trips: window.scaena.trips().length }));
  /** Wait for `fn(arg)` to hold in the page. One that runs out says what it waited for, and the
   * state shown, the nodes the canvas has boxes for, and the status line then (as the first-deck
   * walk's). */
  const until = async (what, fn, arg = null, timeout = 30000) => {
    try {
      await page.waitForFunction(fn, arg, { timeout, polling: 50 });
    } catch {
      const where = await page
        .evaluate(() => ({
          shown: window.scaena.canvas.boxed(),
          nodes: window.scaena.canvas.boxes().map((b) => b.node),
          status: document.querySelector("#status").textContent,
        }))
        .catch(() => ({}));
      throw new Error(`${what}: not within ${timeout / 1000} s · ${where.shown} shown · boxes ${where.nodes} · ${where.status}`);
    }
  };
  /** Once the source is not what it was at `before` and the editor has compiled and linted it. */
  const changed = async (before, what = "the edit compiled", timeout = 60000) => {
    await until(
      what,
      (b) => window.scaena.source() !== b.source && window.scaena.trips().length > b.trips && !window.scaena.canvas.typed()?.sending,
      before,
      timeout,
    );
    await page.waitForTimeout(200);
  };
  /** Once the source is `to` again and compiled. */
  const back = (to, timeout = 60000) =>
    page
      .waitForFunction((s) => window.scaena.source() === s && !window.scaena.canvas.typed()?.sending, to, { timeout, polling: 50 })
      .then(() => true)
      .catch(() => false);
  const states = () => page.evaluate(() => [...document.querySelectorAll("#strip li")].map((li) => li.dataset.state));
  const boxes = () => page.evaluate(() => window.scaena.canvas.boxes());
  /** The page's point at canvas units `[x, y]`. */
  const client = ([x, y]) =>
    page.evaluate(
      ([x, y]) => {
        const r = document.querySelector("#overlay").getBoundingClientRect();
        const [w, h] = window.scaena.canvas.size();
        return [r.left + (x / w) * r.width, r.top + (y / h) * r.height];
      },
      [x, y],
    );
  const middleOf = async (node) => {
    await until(`${node} on the canvas`, (n) => window.scaena.canvas.boxes().some((b) => b.node === n), node);
    const [x, y, w, h] = (await boxes()).find((b) => b.node === node).rect;
    return client([x + w / 2, y + h / 2]);
  };
  /** Show state `id`, as a click on its card in the strip does. */
  const show = async (id) => {
    await page.locator(`#strip li[data-state="${id}"]`).click();
    await until(`state ${id} shown`, (s) => window.scaena.canvas.boxed() === s, id);
    await page.waitForTimeout(300);
  };
  /** The canvas as painted, to tell whether it changed. */
  const painted = async () => decode(await page.locator("#stage").screenshot()).rgba;
  const differ = (a, b) => {
    let n = 0;
    for (let i = 0; i < a.length; i += 4) if (a[i] !== b[i] || a[i + 1] !== b[i + 1] || a[i + 2] !== b[i + 2]) n++;
    return n;
  };
  const step = async (name, act) => {
    const started = Date.now();
    try {
      await act();
    } catch (e) {
      check(false, `${name}: ${String(e).split("\n")[0]}`);
    }
    console.log(`     ${name}: ${((Date.now() - started) / 1000).toFixed(1)} s`);
    await snap(name).catch(() => {});
  };

  let original = "";
  await step("open", async () => {
    await page.goto(editor);
    await page.waitForFunction(() => window.scaena?.last()?.valid && window.scaena.trips().length > 0, null, { timeout: 120000, polling: 50 });
    original = await source();
    const ids = await states();
    check(ids.length === 15 && ids[0] === "cover", `the editor opens the site's demo deck: ${ids.length} states`);
  });

  // 1. The theme picker: Daybreak, then the theme the deck had.
  await step("theme-daybreak", async () => {
    await until("Daybreak in the theme picker", () => [...document.querySelectorAll("#theme option")].some((o) => o.value === "ships:Daybreak"));
    const was = await page.$eval("#theme", (s) => s.value);
    const before = await mark();
    await page.selectOption("#theme", "ships:Daybreak");
    await changed(before);
    check(/daybreak/i.test((await source()).split("\n").find((l) => l.startsWith("deck ")) ?? ""), `the deck is in Daybreak: ${await status()}`);
    // In Daybreak the storm slide sets its title and note dark on its dark photo: each contrast
    // error offers the theme's color that reads there, taken in one click (PLAN 2.82), and undone.
    await show("storm");
    const themed = await source();
    for (const node of ["storm-title", "storm-note"]) {
      await until(`a contrast error on ${node}`, (n) => window.scaena.canvas.marks().some((m) => m.node === n && m.severity === "error"), node, 60000);
      await page.locator(`#marks [data-mark="${node}"]`).click();
      await until(`${node}'s findings open`, () => document.querySelector("#marked").matches(":popover-open"));
      const fixes = await page.$$eval("#marked [data-fix]", (bs) => bs.map((b) => ({ i: b.dataset.fix, change: b.nextElementSibling?.textContent ?? "" })));
      const recolor = fixes.find((f) => f.change.includes("color: surface"));
      if (!check(Boolean(recolor), `${node}'s contrast error offers the theme's surface color: ${JSON.stringify(fixes)}`)) continue;
      const at = await mark();
      await page.locator(`#marked [data-fix="${recolor.i}"]`).click();
      await changed(at);
      const clear = (n) => !window.scaena.canvas.marks().some((m) => m.node === n && m.severity === "error");
      await until(`${node} reading, its fix taken`, clear, node, 60000);
      check(await page.evaluate(clear, node), `one click, and ${node} reads: no error on it`);
    }
    for (const _ of ["storm-note", "storm-title"]) {
      const at = await mark();
      await page.locator("#overlay").focus();
      await page.keyboard.press("Control+z");
      await changed(at);
    }
    check(await back(themed), "and two undos take both fixes back");
    await until(`${was} in the theme picker`, (w) => [...document.querySelectorAll("#theme option")].some((o) => o.value === w), was);
    const again = await mark();
    await page.selectOption("#theme", was);
    await changed(again);
    const deckLine = (await source()).split("\n").find((l) => l.startsWith("deck ")) ?? "";
    check(/dusk/i.test(deckLine), `and back in the theme it had: ${deckLine}`);
  });

  // 2. The strip: move, rename, delete, undo.
  await step("strip", async () => {
    const before = await states();
    await page.locator('#strip li[data-state="agenda"]').click();
    let at = await mark();
    await page.keyboard.press("Alt+ArrowRight");
    await changed(at);
    const moved = await states();
    check(moved.indexOf("agenda") === before.indexOf("agenda") + 1, `Alt+→ moves a slide one on: ${moved.slice(0, 4).join(", ")}`);
    await page.locator('#strip li[data-state="hours"]').click();
    await page.keyboard.press("F2");
    const field = page.locator("#strip li input");
    await field.waitFor({ timeout: 10000 });
    at = await mark();
    await field.fill("volunteer-hours");
    await field.press("Enter");
    await changed(at);
    check((await states()).includes("volunteer-hours") && !(await states()).includes("hours"), "F2 renames a slide");
    await page.locator('#strip li[data-state="voice"]').click();
    at = await mark();
    await page.keyboard.press("Delete");
    await changed(at);
    const less = await states();
    check(!less.includes("voice") && less.length === before.length - 1, `Delete takes a slide away: ${less.length} states`);
    const deleted = await mark();
    await page.locator('#strip li[data-state="process"]').click();
    await page.keyboard.press("Control+z");
    await changed(deleted);
    check((await states()).includes("voice"), `⌘Z puts it back: ${await status()}`);
  });

  // 3. A text on the canvas: another role, and another color.
  await step("inspector", async () => {
    await show("promise");
    const [x, y] = await middleOf("promise-sub");
    await page.mouse.click(x, y);
    await until("promise-sub selected", () => window.scaena.canvas.selected() === "promise-sub");
    await until("the inspector's roles", () => document.querySelectorAll("#look-role option").length > 1);
    const roles = await page.$$eval("#look-role option", (o) => o.map((x) => x.value).filter(Boolean));
    let at = await mark();
    await page.selectOption("#look-role", "headline");
    await changed(at);
    check(/promise-sub[^\n]*role:headline/.test(await source()), `the inspector sets its role: headline, of ${roles.join(", ")}`);
    await until("the inspector's colors", () => document.querySelectorAll("#look-style-color option").length > 1);
    // Another color than the one it has.
    const colors = await page.$$eval("#look-style-color option", (o) => o.filter((x) => x.value && !x.selected).map((x) => x.value));
    const color = colors.find((c) => c === "accent-2") ?? colors[0];
    const written = (text) => (text.match(new RegExp(`color: ${color}\\b`, "g")) ?? []).length;
    const was = written(await source());
    at = await mark();
    await page.selectOption("#look-style-color", color);
    await changed(at);
    check(written(await source()) > was, `and its color: ${color} · ${await status()}`);
  });

  // 4. A cell in the Data tab, and the chart that reads it drawn again.
  await step("data-cell", async () => {
    await show("miles");
    await page.waitForTimeout(500);
    const was = await painted();
    await page.click("#tab-data");
    await until("the Data tab's sources", () => [...document.querySelectorAll("#data select[data-source] option")].some((o) => o.value === "miles"));
    await page.selectOption("#data select[data-source]", "miles");
    // The rows, not the table: the table is in the page before the worker's rows come.
    await until("miles' rows in the Data tab", () => document.querySelector("#data [data-where]")?.textContent.includes("trails-miles") && document.querySelector('#data input[data-row="0"]'));
    const columns = await page.$$eval("#data thead th", (th) => th.map((t) => t.textContent));
    const number = columns.findIndex((c, i) => i > 0 && c.endsWith("number")) - 1;
    const cell = page.locator(`#data input[data-row="0"][data-col="${number}"]`);
    const held = await cell.inputValue();
    // A small change: the bar grows and its annotation stays clear of it.
    const set = Math.round((Number(held) + 0.4) * 10) / 10;
    await cell.fill(String(set));
    await cell.press("Enter");
    await until("the cell set", () => / of row 0 set/.test(document.querySelector("#status").textContent));
    // The chart drawn again: the canvas as painted is not what it was.
    let now = await painted();
    for (let i = 0; i < 40 && differ(was, now) <= 100; i++) {
      await page.waitForTimeout(250);
      now = await painted();
    }
    check(differ(was, now) > 100, `a cell set (${columns[number + 1]} of row 0: ${held} → ${set}) draws the chart again: ${differ(was, now)} pixels changed · ${await status()}`);
  });

  // 5. A figure quoted from the Data tab into the title's characters.
  await step("quote", async () => {
    const [x, y] = await middleOf("miles-title");
    await page.mouse.dblclick(x, y);
    await until("typing in miles-title", () => window.scaena.canvas.typing() === "miles-title");
    // "June to September beat the plan": the first word, by the keys.
    await page.keyboard.press("Home");
    for (let i = 0; i < 4; i++) await page.keyboard.press("Shift+ArrowRight");
    await page.click("#tab-data");
    const cell = page.locator('#data input[data-row="0"][data-col="1"]');
    await cell.click();
    const at = await mark();
    await page.click("#data [data-quote]");
    await changed(at);
    check((await source()).includes("quote: {data: @miles"), `Quote makes the characters quote the cell: ${await status()}`);
    // The beat's claim says "June to September…" too: words first quoted were no figure, and the
    // claim keeps them.
    check((await source()).includes('beat pace "June to September beat the plan."'), "the beat's claim keeps the words the quote took the place of");
    await page.locator("#overlay").focus();
    await page.keyboard.press("Escape");
    await page.click("#tab-inspector");
  });

  // 6. ⌘F: a word across the deck, every match replaced.
  await step("find-replace", async () => {
    await page.locator("#overlay").focus();
    await page.keyboard.press("Escape");
    await page.keyboard.press("Control+f");
    const find = page.locator('#find input[name="find"]');
    await find.waitFor({ timeout: 10000 });
    await find.fill("Alpine");
    await until("find's count", () => /\d+ match/.test(document.querySelector("#find output").textContent));
    const said = await page.textContent("#find output");
    const count = Number(said.match(/(\d+) match/)?.[1] ?? 0);
    const before = ((await source()).match(/Summit/g) ?? []).length;
    // Every "Alpine" in the deck's words; a chart's annotation that names the data's Alpine series
    // is no word, and stays.
    const words = (text) => text.split("\n").filter((l) => !/series/.test(l)).join("\n");
    const alpine = (words(await source()).match(/Alpine/g) ?? []).length;
    // A word of the same length: what fits now fits after.
    await page.fill('#find input[name="replace"]', "Summit");
    const at = await mark();
    await page.click('#find [data-find="all"]');
    await changed(at);
    const after = await source();
    // Every "Alpine" the deck says, in its texts, its claims, and its descriptions (PLAN 2.83), and
    // none left but the series the annotation names.
    check(count === alpine && (after.match(/Summit/g) ?? []).length - before === count && !/Alpine/.test(words(after)) && /Alpine/.test(after), `⌘F finds "Alpine" everywhere the deck says it (${said}, of ${alpine} in its words), and Replace All replaces all ${count}, the data's series named as it was`);
    await page.click('#find [data-find="close"]');
  });

  // 7. The deck in 9:16 (PLAN 2.84): its process slide's steps one above another there.
  await step("formats", async () => {
    const offered = await page.$$eval("#format option", (o) => o.map((x) => x.value).filter(Boolean));
    check(offered.includes("9:16"), `the format picker offers the deck's formats: ${offered.join(", ")}`);
    await show("process");
    const box = (node) => page.evaluate((n) => window.scaena.canvas.boxes().find((b) => b.node === n)?.rect, node);
    const ownTitle = await box("process-title");
    await page.selectOption("#format", "9:16");
    await until("the canvas in 9:16", () => {
      const [w, h] = window.scaena.canvas.size();
      return w === 1080 && h === 1920 && window.scaena.canvas.boxes().some((b) => b.node === "process-plan");
    });
    const [survey, plan] = [await box("process-survey"), await box("process-plan")];
    check(Math.abs(survey[0] - plan[0]) < 1 && plan[1] > survey[1] + survey[3] - 1, `in 9:16 the steps stand one above another: ${JSON.stringify([survey, plan])}`);
    await until("every state linted", () => window.scaena.last()?.whole, null, 60000);
    const there = await page.evaluate(() => window.scaena.last().findings.filter((f) => f.format === "9:16").map((f) => `${f.code} ${f.message}`));
    check(!there.length, `lint finds nothing in 9:16${there.length ? `: ${there.join("; ")}` : ""}`);
    // A move in 9:16 moves a node there alone (PLAN 2.85): the title, laid out anew there, a
    // row down by the arrow key, as a drag would end.
    const tall = await box("process-title");
    const [tx, ty] = await middleOf("process-title");
    await page.mouse.click(tx, ty);
    await until("process-title selected", () => window.scaena.canvas.selected() === "process-title");
    let at = await mark();
    await page.keyboard.press("ArrowDown");
    await changed(at, "the title moved in 9:16");
    await until("the title's new box", (y) => window.scaena.canvas.boxes().find((b) => b.node === "process-title")?.rect[1] > y, tall[1]);
    // Places anew from the right-click menu: the budget's note, which has no layout of its own
    // in 9:16, then has one, and the menu no longer offers it.
    await show("budget");
    await until("budget in 9:16", () => window.scaena.canvas.boxes().some((b) => b.node === "budget-note"));
    const menu = async (node) => {
      const [x, y] = await middleOf(node);
      await page.mouse.click(x, y, { button: "right" });
      await until("the menu", () => document.querySelector(".context-menu"));
      const items = await page.$$eval(".context-menu [role=menuitem]", (b) => b.map((x) => x.textContent.trim()));
      return items;
    };
    const offering = await menu("budget-note");
    check(offering.includes("Place anew in this format"), `in 9:16, a right click on the note offers to place it anew there: ${offering.join(", ")}`);
    at = await mark();
    await page.evaluate(() =>
      [...document.querySelectorAll(".context-menu [role=menuitem]")].find((b) => b.textContent.trim() === "Place anew in this format").click(),
    );
    await changed(at, "the note placed anew");
    const after = await menu("budget-note");
    await page.keyboard.press("Escape");
    check(!after.includes("Place anew in this format") && /budget-note[\s\S]*?formats:\{"9:16"/.test(await source()), `and then has a layout of its own there: ${await status()}`);
    // ⌘D on a node with a layout of its own in 9:16 puts the copy clear of it in 9:16, and on
    // the deck's own canvas too (PLAN 2.86).
    const apart = (a, b) => a[0] >= b[0] + b[2] || b[0] >= a[0] + a[2] || a[1] >= b[1] + b[3] || b[1] >= a[1] + a[3];
    const [wx, wy] = await middleOf("budget-why");
    await page.mouse.click(wx, wy);
    await until("budget-why selected", () => window.scaena.canvas.selected() === "budget-why");
    const unduplicated = await source();
    at = await mark();
    await page.keyboard.press("Control+d");
    await changed(at, "the copy made");
    await until("the copy selected", () => window.scaena.canvas.selected()?.startsWith("budget-why-"));
    const copy = await page.evaluate(() => window.scaena.canvas.selected());
    const tallApart = apart(await box("budget-why"), await box(copy));
    await page.selectOption("#format", "");
    await until("the deck's own canvas again", () => window.scaena.canvas.size()[0] === 1920);
    await show("budget");
    await until("the copy on the deck's own canvas", (c) => window.scaena.canvas.boxes().some((b) => b.node === c), copy);
    check(tallApart && apart(await box("budget-why"), await box(copy)), `⌘D in 9:16 puts ${copy} clear of budget-why there and on the deck's own canvas`);
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+z");
    check(await back(unduplicated), "and ⌘Z takes the copy away");
    await show("process");
    const own = await box("process-title");
    check(JSON.stringify(own) === JSON.stringify(ownTitle), `on the deck's own canvas the title stands where it stood: ${JSON.stringify(own)}`);
  });

  // 8. Rehearse two slides, and keep their times.
  await step("rehearse", async () => {
    await show("cover");
    await page.click("#rehearse-open");
    await page.waitForSelector("#rehearsing:not([hidden])", { timeout: 30000 });
    await page.waitForTimeout(1500);
    await page.click('#rehearsing [data-on]');
    await page.waitForTimeout(1500);
    await page.click('#rehearsing [data-on]');
    await page.waitForTimeout(500);
    await page.click('#rehearsing [data-end]');
    await page.waitForSelector("#rehearsed[open]", { timeout: 30000 });
    const rows = await page.$$eval("#rehearsed tbody tr", (tr) => tr.map((r) => r.textContent.replace(/\s+/g, " ").trim()));
    const at = await mark();
    await page.click("#rehearsed-keep");
    await changed(at);
    check(rows.length >= 2 && /hold:/.test(await source()), `Rehearse times two slides, and Keep makes them holds: ${rows.slice(0, 2).join(" | ")}`);
  });

  // 9. Play, and the presenter view, which steers the player.
  await step("present", async () => {
    await page.click("#save");
    await page.waitForFunction(() => window.scaena.where().where && !window.scaena.where().dirty, null, { timeout: 60000 }).catch(() => {});
    const [player] = await Promise.all([context.waitForEvent("page", { timeout: 30000 }), page.click("#play")]);
    player.on("pageerror", (e) => failures.push(`player: ${e.message}`));
    await player.waitForFunction(() => window.scaena?.at && /CPU painter/.test(document.querySelector("#status")?.textContent ?? ""), null, { timeout: 120000 });
    const [presenter] = await Promise.all([context.waitForEvent("page", { timeout: 30000 }), player.click("#present")]);
    presenter.on("pageerror", (e) => failures.push(`presenter: ${e.message}`));
    await presenter.waitForSelector("#on", { timeout: 120000 });
    await presenter.waitForTimeout(1500);
    const was = await player.evaluate(() => window.scaena.at().index);
    await presenter.click("#on");
    const went = await player
      .waitForFunction((w) => window.scaena.at().index === w + 1, was, { timeout: 30000 })
      .then(() => true)
      .catch(() => false);
    // The presenter view follows the player to where it went.
    const follows = await presenter
      .waitForFunction((w) => new RegExp(`· ${w + 2} / `).test(document.body.textContent), was, { timeout: 30000 })
      .then(() => true)
      .catch(() => false);
    check(follows, "and the presenter view shows where the player went");
    await snap("presenter", presenter);
    await snap("player", player);
    check(went, `the presenter view goes on, and the player with it: ${await player.evaluate(() => document.querySelector("#status").textContent)}`);
    await presenter.close();
    await player.close();
  });

  // 10. A PDF, and the .scaena.
  const downloaded = async (act) => {
    const [download] = await Promise.all([page.waitForEvent("download", { timeout: 180000 }), act()]);
    const bytes = await readFile(await download.path());
    await writeFile(join(out, download.suggestedFilename()), bytes);
    return { name: download.suggestedFilename(), bytes };
  };
  await step("export-pdf", async () => {
    await page.click("#export");
    await page.check("#exporting input[name=as][value=pdf]");
    const pdf = await downloaded(() => page.click("#exporting-go"));
    check(pdf.name.endsWith(".pdf") && pdf.bytes.subarray(0, 5).toString() === "%PDF-", `Export… downloads ${pdf.name}, ${pdf.bytes.length} B`);
  });
  await step("download", async () => {
    const zip = await downloaded(() => page.click("#download"));
    check(zip.name.endsWith(".scaena") && zip.bytes[0] === 0x50 && zip.bytes[1] === 0x4b, `Download gives ${zip.name}, ${zip.bytes.length} B`);
  });

  await step("findings", async () => {
    // Once typing stops, every state is linted: what the deck has is what that finds. The status
    // line says so only until the next thing it says; the lint itself is what to wait for.
    await until("every state linted", () => window.scaena.last()?.whole, null, 60000);
    const found = await page.evaluate(() => window.scaena.last().findings.map((f) => `${f.severity} ${f.code} ${f.message}`));
    console.log(`     lint: ${found.join("; ") || "nothing"}`);
    check(!found.some((f) => f.startsWith("error")), `lint finds no error in the edited deck: ${found.filter((f) => f.startsWith("error")).join("; ")}`);
  });
  void original;
} catch (e) {
  failures.push(String(e));
} finally {
  await browser.close();
  server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "a deck edited: re-themed, rearranged, restyled, its data and words changed, rehearsed, presented, and exported");
process.exit(failures.length ? 1 : 0);

// A first deck, as a person makes one (PLAN 2.76): the editor in headless Chromium (serve.mjs),
// driven by its buttons, menus, keys, and drops alone, from New to a deck that plays, exports,
// and opens again. The CPU paints, as in web/editor.mjs. `window.scaena` is only read, to wait
// and to check; nothing is done through it.
//
//   node web/first-deck.mjs     (after `just web`; from the repository's root)
//
// - New, from its dialog: a deck in Dusk with one empty state.
// - A title slide: the state's layout from the inspector, a text from Insert in the title slot,
//   its words typed where it stands.
// - + Slide, then a body text typed as three lines and made a bulleted list with ⌘⇧8.
// - A CSV dropped on the slide becomes the deck's data source, and a chart of it lands there; the
//   same file again is another chart of it, not a second source, and one undo takes it out; the
//   Data tab shows its rows.
// - + Slide, and a photo dropped on it is put there.
// - A motion from the cue's menu on the title.
// - Every state linted: no error.
// - Save keeps it in the browser, and Play plays it in a tab of its own.
// - Export… gives a PDF and one HTML file; Download gives the .scaena, which opens again as it was.
//
// Each step is screenshotted into target/first-deck/, with its time. Exits 1 on any failure.
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { launch, serve } from "./serve.mjs";

const out = "target/first-deck";
await mkdir(out, { recursive: true });
const server = await serve();
const editor = `${server.origin}/web/dist/editor.html?painter=cpu`;
const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
  return ok;
};

const csv = "month,visits\n2026-04,1200\n2026-05,1850\n2026-06,2900\n2026-07,3400\n2026-08,3100\n2026-09,2200\n";
const photo = await readFile("docs/examples/agent-run/dusk.jpg");

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
  /** Once the source is not what it was at `before` and the editor has compiled and linted it. */
  const changed = async (before, timeout = 60000) => {
    await page.waitForFunction(
      (b) => window.scaena.source() !== b.source && window.scaena.trips().length > b.trips && !window.scaena.canvas.typed()?.sending,
      before,
      { timeout, polling: 50 },
    );
    await page.waitForTimeout(200);
  };
  const boxes = () => page.evaluate(() => window.scaena.canvas.boxes());
  /** The nodes the state shown shows. */
  const shownNodes = async () => (await boxes()).map((b) => b.node);
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
    await page.waitForFunction((n) => window.scaena.canvas.boxes().some((b) => b.node === n), node, { timeout: 30000 });
    const [x, y, w, h] = (await boxes()).find((b) => b.node === node).rect;
    return client([x + w / 2, y + h / 2]);
  };
  /** Show the `n`th state, as a click on its card in the strip does. */
  const showState = async (n) => {
    await page.locator("#strip li").nth(n).click();
    await page.waitForFunction((n) => window.scaena.shown() === n, n, { timeout: 30000 });
  };
  /** A file dropped on the element `selector` at its point `[x, y]` (the page's), as a drag from
   * the desktop drops it. */
  const dropFile = (selector, at, name, type, bytes) =>
    page.evaluate(
      async ({ selector, at, name, type, bytes }) => {
        const target = document.querySelector(selector);
        const data = new DataTransfer();
        data.items.add(new File([new Uint8Array(bytes)], name, { type }));
        const [clientX, clientY] = at;
        for (const kind of ["dragenter", "dragover", "drop"]) {
          target.dispatchEvent(new DragEvent(kind, { dataTransfer: data, clientX, clientY, bubbles: true, cancelable: true }));
        }
      },
      { selector, at, name, type, bytes: [...bytes] },
    );
  /** Double click `node` to type in it, select all its words, and type `lines`, Enter between. */
  const typeInto = async (node, lines) => {
    const [x, y] = await middleOf(node);
    await page.mouse.dblclick(x, y);
    await page.waitForFunction((n) => window.scaena.canvas.typing() === n, node, { timeout: 30000 });
    await page.keyboard.press("Control+a");
    const before = await mark();
    for (const [i, line] of lines.entries()) {
      if (i) await page.keyboard.press("Enter");
      await page.keyboard.type(line, { delay: 15 });
    }
    await changed(before);
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

  await step("open", async () => {
    await page.goto(editor);
    await page.waitForFunction(() => window.scaena?.last(), null, { timeout: 120000 });
  });

  await step("new", async () => {
    await page.click("#new-deck");
    await page.fill("#making-name", "Trail report");
    await page.selectOption("#making-theme", "Dusk");
    await page.click("#making-create");
    await page.waitForFunction(() => window.scaena.where().name === "trail-report" && window.scaena.last(), null, { timeout: 120000 });
    check((await shownNodes()).length === 0, "New makes a deck with one empty state");
  });

  await step("title-layout", async () => {
    await page.keyboard.press("Escape");
    await page.waitForSelector("#look-layout", { timeout: 30000 });
    const layouts = await page.evaluate(() => [...document.querySelectorAll("#look-layout option")].map((o) => o.value));
    check(layouts.includes("title"), `the empty state offers the theme's layouts: ${layouts.filter(Boolean).join(", ")}`);
    const before = await mark();
    await page.selectOption("#look-layout", "title");
    await changed(before);
    check(/state \S+ layout:title/.test(await source()), "the state takes the title layout");
  });

  let title;
  await step("insert-title", async () => {
    await page.waitForFunction(() => document.querySelectorAll("#insert option").length > 1, null, { timeout: 30000 });
    const before = await shownNodes();
    await page.selectOption("#insert", { label: "display" });
    await page.waitForFunction((n) => window.scaena.canvas.boxes().length > n, before.length, { timeout: 60000 });
    title = (await shownNodes()).find((n) => !before.includes(n));
    check(Boolean(title), `Insert puts a display text in the title slot: ${title}`);
  });

  await step("type-title", async () => {
    await typeInto(title, ["Trail report"]);
    await page.keyboard.press("Escape");
    check((await source()).includes('"Trail report"'), "the title reads Trail report where it was typed");
  });

  await step("add-slide", async () => {
    const before = await mark();
    await page.click("#strip [data-add=slide]");
    await changed(before);
    const states = await page.evaluate(() => [...document.querySelectorAll("#strip li")].map((li) => li.dataset.state));
    check(states.length === 2, `+ Slide adds a slide: ${states.join(", ")}`);
    await showState(1);
  });

  await step("insert-list", async () => {
    await page.keyboard.press("Escape");
    const before = await mark();
    await page.selectOption("#look-layout", "full");
    await changed(before);
    const nodes = await shownNodes();
    await page.selectOption("#insert", { label: "body" });
    await page.waitForFunction((n) => window.scaena.canvas.boxes().length > n, nodes.length, { timeout: 60000 });
    const body = (await shownNodes()).find((n) => !nodes.includes(n));
    await typeInto(body, ["Three trails surveyed", "Two need new bridges", "One closes for winter"]);
    const typed = await mark();
    await page.keyboard.press("Control+a");
    await page.keyboard.press("Control+Shift+Digit8");
    await changed(typed);
    await page.keyboard.press("Escape");
    check(/list:\[\{kind: bullet\}, \{kind: bullet\}, \{kind: bullet\}\]/.test(await source()), "⌘⇧8 makes the three lines a bulleted list");
  });

  await step("data-on-canvas", async () => {
    // A person drags a CSV onto the slide: it becomes a data source, and a chart of it lands there.
    const nodes = await shownNodes();
    await dropFile("#overlay", await client([1400, 700]), "visits.csv", "text/csv", Buffer.from(csv));
    await page.waitForFunction((n) => window.scaena.canvas.boxes().length > n, nodes.length, { timeout: 60000 }).catch(() => {});
    const text = await source();
    check(/^data visits "data\/visits\.csv"/m.test(text), `a CSV dropped on the slide becomes the deck's data source @visits: ${await status()}`);
    const chart = (await shownNodes()).find((n) => !nodes.includes(n));
    check(chart === "visits-chart" && /visits-chart chart:line data:@visits/.test(text), `and a chart of it lands where it was dropped: ${chart}`);
  });

  await step("data-again", async () => {
    // The same CSV dropped again is the source that reads it: another chart of @visits, and no
    // second source. One undo takes the chart out.
    const before = await mark();
    const nodes = await shownNodes();
    await dropFile("#overlay", await client([400, 800]), "visits.csv", "text/csv", Buffer.from(csv));
    await changed(before);
    const text = await source();
    const chart = (await shownNodes()).find((n) => !nodes.includes(n));
    check(
      (text.match(/^data /gm) ?? []).length === 1 && new RegExp(`^\\s*${chart} chart:line data:@visits`, "m").test(text),
      `the same CSV again is another chart of @visits, not a second source: ${chart} · ${await status()}`,
    );
    const again = await mark();
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+z");
    await changed(again);
    check((await source()) === before.source, `one undo takes the chart out: ${await status()}`);
  });

  await step("data-tab", async () => {
    await page.click("#tab-data");
    await page.waitForFunction(() => document.querySelector("#data table"), null, { timeout: 30000 }).catch(() => {});
    const rows = await page.evaluate(() => document.querySelectorAll("#data table tbody tr").length);
    const said = await page.evaluate(() => document.querySelector("#data").textContent.replace(/\s+/g, " ").trim().slice(0, 80));
    check(rows === 6 && said.includes("visits"), `the Data tab shows its 6 rows: ${said}`);
    await page.click("#tab-inspector");
  });

  await step("photo-on-canvas", async () => {
    // A slide of its own for a photo, dragged onto it from the desktop: it is put where it lands.
    const before = await mark();
    await page.click("#strip [data-add=slide]");
    await changed(before);
    await showState(2);
    await page.waitForFunction(() => window.scaena.canvas.boxes().length === 0, null, { timeout: 30000 });
    await dropFile("#overlay", await client([960, 540]), "trailhead.jpg", "image/jpeg", photo);
    await page.waitForFunction(() => window.scaena.canvas.boxes().length > 0, null, { timeout: 60000 }).catch(() => {});
    const added = await shownNodes();
    check(added.length === 1 && / image\s+"assets\/[0-9a-f]{64}\.jpg"/.test(await source()), `a photo dropped on the slide is put there: ${added} · ${await status()}`);
  });

  await step("motion", async () => {
    await showState(0);
    const [x, y] = await middleOf(title);
    await page.mouse.click(x, y);
    await page.waitForFunction(() => document.querySelectorAll("#cue [data-add] option").length > 1, null, { timeout: 30000 });
    const options = await page.evaluate(() => [...document.querySelectorAll("#cue [data-add] option")].map((o) => [o.value, o.textContent]));
    const enter = options.find(([v]) => v.includes('"enter"'));
    if (!check(Boolean(enter), `the cue offers a motion for the title: ${options.map((o) => o[1]).join(", ")}`)) return;
    const before = await mark();
    await page.locator("#cue [data-add]").selectOption(enter[0]);
    await changed(before);
    check(/enter:fade/.test(await source()), `the title fades in: ${await status()}`);
  });

  await step("findings", async () => {
    // Once typing stops, every state is linted: what the deck has is what that finds.
    await page.waitForFunction(() => /every state linted/.test(document.querySelector("#status").textContent), null, { timeout: 60000 });
    const found = await page.evaluate(() => window.scaena.last().findings.map((f) => `${f.severity} ${f.code} ${f.message}`));
    console.log(`     lint: ${found.join("; ") || "nothing"}`);
    check(!found.some((f) => f.startsWith("error")), `lint finds no error in any state: ${found.filter((f) => f.startsWith("error")).join("; ")}`);
  });

  await step("save", async () => {
    await page.click("#save");
    await page.waitForFunction(() => window.scaena.where().where && !window.scaena.where().dirty, null, { timeout: 60000 });
    check((await page.evaluate(() => window.scaena.where().where?.kind)) === "opfs", "Save keeps it in the browser");
  });

  await step("play", async () => {
    const [player] = await Promise.all([context.waitForEvent("page", { timeout: 30000 }), page.click("#play")]);
    player.on("pageerror", (e) => failures.push(`player: ${e.message}`));
    await player.waitForFunction(() => /CPU painter/.test(document.querySelector("#status")?.textContent ?? ""), null, { timeout: 120000 });
    await player.waitForTimeout(2000);
    await snap("player-title", player);
    for (const name of ["player-list", "player-photo"]) {
      await player.keyboard.press("ArrowRight");
      await player.waitForTimeout(2000);
      await snap(name, player);
    }
    const at = await player.evaluate(() => document.querySelector("#status").textContent);
    check(/^3 \/ 3/.test(at), `the player plays it to its last slide: ${at}`);
    await player.close();
  });

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
    check(pdf.name === "trail-report.pdf" && pdf.bytes.subarray(0, 5).toString() === "%PDF-", `Export… downloads ${pdf.name}, ${pdf.bytes.length} B`);
  });
  await step("export-html", async () => {
    await page.click("#export");
    await page.check("#exporting input[name=as][value=html]");
    const html = await downloaded(() => page.click("#exporting-go"));
    check(html.name === "trail-report.html" && html.bytes.length > 100000, `Export… downloads ${html.name}, ${html.bytes.length} B`);
  });
  let zip;
  await step("download", async () => {
    zip = await downloaded(() => page.click("#download"));
    check(zip.name === "trail-report.scaena", `Download gives ${zip.name}, ${zip.bytes.length} B`);
  });

  await step("reopen", async () => {
    const kept = await page.evaluate(() => window.scaena.last().states.map((s) => s[0]).join());
    await page.goto(editor);
    await page.waitForFunction(() => window.scaena?.last(), null, { timeout: 120000 });
    // The input is hidden in its label, which is what a person clicks.
    const [chooser] = await Promise.all([page.waitForEvent("filechooser"), page.click("label:has(#open-file)")]);
    await chooser.setFiles({ name: zip.name, mimeType: "application/zip", buffer: zip.bytes });
    // The browser keeps what it opens, under a name of its own beside the deck kept there already.
    await page.waitForFunction(() => /^trail-report/.test(window.scaena.where().name ?? "") && window.scaena.last(), null, { timeout: 120000 });
    const opened = await page.evaluate(() => ({ states: window.scaena.last().states.map((s) => s[0]).join(), source: window.scaena.source() }));
    const same = ["Trail report", "Three trails surveyed", 'data visits "data/visits.csv"', "visits-chart", "enter:fade"].every((t) => opened.source.includes(t));
    check(opened.states === kept && same, `the .scaena opens again as it was: ${opened.states}`);
  });
} catch (e) {
  failures.push(String(e));
} finally {
  await browser.close();
  server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "a first deck, made, played, exported, and opened again");
process.exit(failures.length ? 1 : 0);

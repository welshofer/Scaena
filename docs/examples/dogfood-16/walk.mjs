// W1 dogfood 16: a deck built by hand in the browser's editor, then revised as a person revises one
// after a review. Built from web/first-deck.mjs's steps, in headless Chromium, the CPU painting.
// `window.scaena` is only read, to wait and to say what the page shows. Run from the repository's
// root after `just web`:
//   node docs/examples/dogfood-16/walk.mjs [OUTDIR]
//
// - New, a title typed; two slides in the theme's `bullets` layout, each a headline and a body
//   typed; a By Bullet build on the first; notes on its first state and its last step; a CSV
//   dropped on a blank slide.
// - Saved in the browser, a history kept; the title typed again and saved.
// - The Story's findings, and its fold; a restore from Versions; a row added at the top of the
//   data; the PDF with notes.
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { answering, insert, launch, serve, tab } from "../../../web/serve.mjs";

const out = process.argv[2] ?? "target/dogfood-16";
await mkdir(out, { recursive: true });
const server = await serve();
const editor = `${server.origin}/web/dist/editor.html?painter=cpu`;
const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
  return ok;
};

const csv = "month,visits\n2026-05,410\n2026-06,520\n2026-07,780\n2026-08,910\n2026-09,1040\n";

const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1, acceptDownloads: true });
  const page = await context.newPage();
  page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
  page.on("console", (m) => m.type() === "error" && failures.push(`console: ${m.text()}`));
  page.on("dialog", (d) => d.accept());
  // Changes not saved are asked about in the editor's own dialog (lift-up 20261031 ATTR-3), answered
  // as the confirms it replaced were taken.
  await answering(page);

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
  /** Wait for `fn(arg)` to hold in the page. One that runs out says what it waited for, and the
   * state shown, the nodes the canvas has boxes for, and the status line then. */
  const until = async (what, fn, arg = null, timeout = 30000) => {
    try {
      await page.waitForFunction(fn, arg, { timeout, polling: 50 });
    } catch {
      const where = await page
        .evaluate(() => ({
          shown: window.scaena.shown(),
          nodes: window.scaena.canvas.boxes().map((b) => b.node),
          status: document.querySelector("#status").textContent,
        }))
        .catch(() => ({}));
      throw new Error(`${what}: not within ${timeout / 1000} s · state ${where.shown} shown · boxes ${where.nodes} · ${where.status}`);
    }
  };
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
    await until(`${node} on the canvas`, (n) => window.scaena.canvas.boxes().some((b) => b.node === n), node);
    const [x, y, w, h] = (await boxes()).find((b) => b.node === node).rect;
    return client([x + w / 2, y + h / 2]);
  };
  /** + Slide, then `name`'s slide from its gallery of the theme's layouts, as a click there starts
   * it (PLAN 3.30). */
  const addSlide = async (name) => {
    await page.click("#strip [data-add=slide]");
    await page.waitForSelector("#starting[open] button[data-start]", { timeout: 60000 });
    await page.locator("#starting").getByRole("button", { name, exact: true }).click();
  };
  /** Show the `n`th state, as a click on its card in the strip does. */
  const showState = async (n) => {
    await page.locator("#strip li").nth(n).click();
    await until(`state ${n} shown`, (n) => window.scaena.shown() === n, n);
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
      await snap(`${name}-failed`).catch(() => {});
    }
    console.log(`     ${name}: ${((Date.now() - started) / 1000).toFixed(1)} s`);
    await snap(name).catch(() => {});
  };


  const notesShown = () => page.evaluate(() => document.querySelector("#look-notes")?.value ?? null);
  const said = (...a) => console.log("     " + a.map((x) => (typeof x === "string" ? x : JSON.stringify(x))).join(" "));
  const states = () => page.evaluate(() => window.scaena.last().states.map((s) => s[0]));
  const stateIndex = async (id) => (await states()).indexOf(id);
  const showId = async (id) => showState(await stateIndex(id));

  await step("open", async () => {
    await page.goto(editor);
    await page.waitForFunction(() => window.scaena?.last(), null, { timeout: 120000 });
  });

  let title, subtitle;
  await step("new", async () => {
    await page.click("#file-open").then(() => page.click("#new-deck"));
    await page.fill("#making-name", "Library report");
    await page.click("#making-themes .tile:has(input[value=Dusk])");
    await page.click("#making-create");
    await page.waitForFunction(() => window.scaena.where().name === "library-report" && window.scaena.last(), null, { timeout: 120000 });
    const text = await source();
    const placed = (slot) => new RegExp(`^\\s*(\\S+) text .*at:in\\(${slot}\\)`, "m").exec(text)?.[1];
    [title, subtitle] = [placed("title"), placed("subtitle")];
    await typeInto(title, ["Our library at night"]);
    await page.keyboard.press("Escape");
    await typeInto(subtitle, ["Branch report, autumn 2026"]);
    await page.keyboard.press("Escape");
    check((await source()).includes('"Our library at night"'), "the title typed");
  });

  /** + Slide Blank, laid out in the theme's `bullets`, its headline and body inserted and typed. */
  const bodySlide = async (headline, lines) => {
    let before = await mark();
    await addSlide("Blank");
    await changed(before);
    const n = (await states()).length - 1;
    await showState(n);
    before = await mark();
    await page.click("#look-layout");
    await page.waitForSelector("#starting[open] [role=tab]", { timeout: 60000 });
    await page.fill("#starting .find", "bullets");
    await page.click('#starting button[data-start="bullets"]');
    await changed(before);
    const added = async (label) => {
      const nodes = await shownNodes();
      await insert(page, label);
      await page.waitForFunction((n) => window.scaena.canvas.boxes().length > n, nodes.length, { timeout: 60000 });
      return (await shownNodes()).find((n) => !nodes.includes(n));
    };
    const head = await added("Headline");
    await typeInto(head, [headline]);
    await page.keyboard.press("Escape");
    const body = await added("Body");
    await typeInto(body, lines);
    await page.keyboard.press("Escape");
    return { head, body, state: (await states())[n] };
  };

  let hours;
  await step("hours-slide", async () => {
    hours = await bodySlide("We now open late four nights a week", ["Open until 10 pm four nights a week", "Quiet floor stays quiet", "Study rooms book online"]);
    said("hours slide", hours);
  });

  await step("by-bullet", async () => {
    await showId(hours.state);
    await page.evaluate((n) => window.scaena.canvas.select(n), hours.body);
    await page.waitForFunction((n) => window.scaena.look.offered()?.node === n, hours.body, { timeout: 30000 });
    await tab(page, "animate");
    await page.waitForSelector('#look-enter option[value="bullets"]', { state: "attached", timeout: 30000 });
    const before = await mark();
    await page.selectOption("#look-enter", "bullets");
    await changed(before);
    const all = await states();
    said("states after By Bullet", all.join(" "), "·", await status());
    check(all.filter((s) => s.startsWith(hours.state)).length === 3, "By Bullet adds two steps");
  });

  await step("step-notes", async () => {
    // Notes on the slide's first state, written with nothing selected, as a person writes them.
    await page.click("#tab-inspector");
    await showId(hours.state);
    await page.keyboard.press("Escape");
    await page.waitForSelector("#look-notes", { timeout: 30000 });
    let before = await mark();
    await page.fill("#look-notes", "We stayed open late because students asked us to.");
    await page.locator("#look-notes").blur();
    await changed(before).catch(() => {});
    const steps = (await states()).filter((s) => s.startsWith(hours.state));
    for (const s of steps) {
      await showId(s);
      await page.keyboard.press("Escape");
      await page.waitForTimeout(400);
      said(`notes shown on ${s}:`, await notesShown());
    }
    // The last step given notes of its own.
    await showId(steps[2]);
    await page.keyboard.press("Escape");
    before = await mark();
    await page.fill("#look-notes", "Then the study rooms: booked out every exam week.");
    await page.locator("#look-notes").blur();
    await changed(before).catch(() => {});
    const text = await source();
    said("notes in the source:", text.split("\n").filter((l) => /notes:/.test(l)).map((l) => l.trim().slice(0, 140)));
    await showId(steps[0]);
    await page.keyboard.press("Escape");
    await page.waitForTimeout(400);
    said("first state's notes after the step's edit:", await notesShown());
  });

  await step("chart-slide", async () => {
    const before = await mark();
    await addSlide("Blank");
    await changed(before);
    await showState((await states()).length - 1);
    const nodes = await shownNodes();
    await dropFile("#overlay", await client([960, 540]), "visits.csv", "text/csv", Buffer.from(csv));
    await page.waitForFunction((n) => window.scaena.canvas.boxes().length > n, nodes.length, { timeout: 60000 }).catch(() => {});
    check(/^data visits "data\/visits\.csv"/m.test(await source()), `the CSV is the deck's data: ${await status()}`);
  });

  let again;
  await step("again-slide", async () => {
    // A slide that says again what the hours slide said, as a draft often does.
    again = await bodySlide("Late opening four nights a week", ["Open until 10 pm four nights a week", "Students asked for late hours", "Quiet floor stays quiet"]);
  });

  await step("save-and-keep", async () => {
    await page.click("#save");
    await page.waitForFunction(() => window.scaena.where().where && !window.scaena.where().dirty, null, { timeout: 60000 });
    await tab(page, "versions");
    await page.waitForFunction(() => window.scaena.versions.listed() !== undefined, null, { timeout: 60000 }).catch(() => {});
    await page.waitForFunction(() => document.querySelector("#versions [data-summary]")?.textContent.trim(), null, { timeout: 60000 }).catch(() => {});
    const keep = page.locator("#versions [data-keep]");
    said("versions summary:", await page.locator("#versions [data-summary]").textContent(), "keep shown:", await keep.isVisible());
    if (await keep.isVisible()) await keep.click();
    await page.waitForFunction(() => window.scaena.versions.listed()?.length > 0, null, { timeout: 60000 }).catch(() => {});
    said("versions:", await page.evaluate(() => window.scaena.versions.listed()?.map((v) => `${v.n} ${v.said}`)));
    await page.click("#tab-inspector").catch(() => {});
  });

  await step("retitle-and-save", async () => {
    await showState(0);
    await typeInto(title, ["Our library after dark"]);
    await page.keyboard.press("Escape");
    await page.click("#save");
    await page.waitForFunction(() => !window.scaena.where().dirty, null, { timeout: 60000 });
  });

  await step("story", async () => {
    await tab(page, "story");
    await page.waitForFunction(() => window.scaena.story.health() && document.querySelector("#story [data-findings] li"), null, { timeout: 120000 });
    const h = await page.evaluate(() => window.scaena.story.health());
    said("story score:", h.score, "findings:", h.findings.length);
    for (const f of h.findings.slice(0, 12)) said(`  [${f.kind}] slide ${f.slide}: ${f.says} → ${f.ask}`);
    const folds = await page.locator("#story button", { hasText: /^Fold into/ }).allTextContents();
    said("fold offers:", folds);
    const fixes = await page.locator("#story [data-findings] button").allTextContents();
    said("story buttons:", [...new Set(fixes)].slice(0, 20));
  });

  await step("fold", async () => {
    const fold = page.locator("#story button", { hasText: /^Fold into/ }).first();
    if (!(await fold.count())) return check(false, "the Story offers no fold for the slide that says the hours again");
    const before = await mark();
    await fold.click();
    await changed(before).catch(() => {});
    said("after fold:", await status(), "states:", (await states()).join(" "));
    const text = await source();
    said("notes after fold:", text.split("\n").filter((l) => /notes:/.test(l)).map((l) => l.trim().slice(0, 160)));
  });

  await step("restore", async () => {
    await page.click("#save").catch(() => {});
    await page.waitForFunction(() => !window.scaena.where().dirty, null, { timeout: 60000 }).catch(() => {});
    await tab(page, "versions");
    await page.waitForFunction(() => window.scaena.versions.listed()?.length > 0, null, { timeout: 60000 });
    const versions = await page.evaluate(() => window.scaena.versions.listed());
    said("versions before restore:", versions.map((v) => `${v.n} ${v.said}`));
    await page.click(`#versions [data-id="${versions[0].id}"]`);
    await page.waitForFunction(() => window.scaena.versions.settled(), null, { timeout: 60000 }).catch(() => {});
    said("restore offers:", await page.locator("#versions button").allTextContents());
    const before = await mark();
    await page.click("#versions [data-restore]");
    await changed(before).catch(() => {});
    const text = await source();
    said("after restore:", await status(), "title back:", text.includes('"Our library at night"'), "fold kept:", (await states()).join(" "));
  });

  await step("data-row-at-top", async () => {
    await tab(page, "data");
    await page.waitForFunction(() => document.querySelectorAll("#data table tbody tr").length > 0, null, { timeout: 30000 });
    const first = page.locator("#data table tbody tr").first().locator("input").first();
    await first.click();
    const before = await page.evaluate(() => window.scaena.data.shown()?.sheet.rows.map((r) => r[0]));
    await page.click("#data [data-add]");
    await page.waitForFunction((n) => window.scaena.data.shown()?.sheet.rows.length === n + 1, before.length, { timeout: 60000 }).catch(() => {});
    const after = await page.evaluate(() => window.scaena.data.shown()?.sheet.rows.map((r) => r[0]));
    said("rows before:", before, "after + Row on the first:", after, "·", await status());
    said("data buttons:", await page.locator("#data button").allTextContents());
  });

  await step("pdf-notes", async () => {
    await page.click("#export");
    await page.check("#exporting input[name=as][value=pdf]");
    await page.check("#exporting-with-notes");
    said("note:", await page.evaluate(() => document.querySelector("#exporting-note")?.textContent));
    const [download] = await Promise.all([page.waitForEvent("download", { timeout: 180000 }), page.click("#exporting-go")]);
    const bytes = await readFile(await download.path());
    await writeFile(join(out, download.suggestedFilename()), bytes);
    said("pdf:", download.suggestedFilename(), bytes.length);
  });
} catch (e) {
  failures.push(String(e));
} finally {
  await browser.close();
  server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "dogfood 16 walked");

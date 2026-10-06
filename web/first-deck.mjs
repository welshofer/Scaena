// A first deck, as a person makes one (the first-deck walk): the editor in headless Chromium
// (serve.mjs), driven by its buttons, menus, keys, and drops alone, from New to a deck that
// plays and exports. The CPU paints, as in web/editor.mjs. `window.scaena` is only read, to wait
// and to check; nothing is done through it.
//
//   node web/first-deck.mjs     (after `just web`; from the repository's root)
//
// Each step is screenshotted into target/first-deck/. Exits 1 on any failure.
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
  const snap = (name) => page.screenshot({ path: join(out, `${String(++shot).padStart(2, "0")}-${name}.png`) });
  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  const source = () => page.evaluate(() => window.scaena.source());
  /** The editor once it has compiled and linted the bundle named `name`. */
  const ready = async (name) => {
    await page.waitForFunction(
      (name) => (window.scaena?.last() && window.scaena.where().name === name) || document.querySelector("#status")?.textContent.startsWith("error"),
      name,
      { timeout: 120000 },
    );
    const said = await page.evaluate(() => (window.scaena?.last() ? "" : document.querySelector("#status").textContent));
    if (said) throw new Error(said);
  };
  /** Once the source is not `before` and the worker has compiled what it is. */
  const changed = async (before, timeout = 60000) => {
    await page.waitForFunction((b) => window.scaena.source() !== b, before, { timeout, polling: 50 });
    await page.waitForFunction(() => !window.scaena.canvas.typed()?.sending, null, { timeout, polling: 50 }).catch(() => {});
    await page.waitForFunction(
      () => window.scaena.trips().length > 0 && window.scaena.last()?.source === window.scaena.source(),
      null,
      { timeout, polling: 50 },
    ).catch(() => {});
    await page.waitForTimeout(300);
  };
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
    await page.waitForFunction((n) => window.scaena.canvas.boxes().some((b) => b.node === n), node, { timeout: 30000 });
    const [x, y, w, h] = (await boxes()).find((b) => b.node === node).rect;
    return client([x + w / 2, y + h / 2]);
  };
  /** The nodes the state shown shows, by id. */
  const shownNodes = async () => (await boxes()).map((b) => b.node);
  /** A file dropped on the element `selector` at its point `[x, y]` (the page's), as a desktop
   * drag drops it. */
  const dropFile = (selector, at, name, type, bytes) =>
    page.evaluate(
      async ({ selector, at, name, type, bytes }) => {
        const target = document.querySelector(selector);
        const file = new File([new Uint8Array(bytes)], name, { type });
        const data = new DataTransfer();
        data.items.add(file);
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
    const before = await source();
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
    await ready("trail-report");
    check((await shownNodes()).length === 0, "New makes a deck with one empty state");
  });

  await step("title-layout", async () => {
    await page.keyboard.press("Escape");
    await page.waitForSelector("#look-layout", { timeout: 30000 });
    const offered = await page.evaluate(() => [...document.querySelectorAll("#look-layout option")].map((o) => o.value));
    check(offered.includes("title"), `the empty state offers the theme's layouts: ${offered.join(", ")}`);
    const before = await source();
    await page.selectOption("#look-layout", "title");
    await changed(before);
    check(/state \S+ layout:title/.test(await source()), "the state takes the title layout");
  });

  let title;
  await step("insert-title", async () => {
    await page.waitForFunction(() => document.querySelectorAll("#insert option").length > 1, null, { timeout: 30000 });
    const offered = await page.evaluate(() => [...document.querySelectorAll("#insert option")].map((o) => o.textContent));
    console.log(`     Insert offers: ${offered.join(" | ")}`);
    const label = offered.find((o) => o === "display") ?? offered.find((o) => o === "title") ?? offered.find((o) => o === "headline");
    const before = await shownNodes();
    await page.selectOption("#insert", { label });
    await page.waitForFunction((n) => window.scaena.canvas.boxes().length > n, before.length, { timeout: 60000 });
    title = (await shownNodes()).find((n) => !before.includes(n));
    check(Boolean(title), `Insert puts a ${label} text on the slide: ${title}`);
  });

  await step("type-title", async () => {
    await typeInto(title, ["Trail report"]);
    await page.keyboard.press("Escape");
    check((await source()).includes('"Trail report"'), "the title reads Trail report");
  });

  await step("add-slide", async () => {
    const before = await source();
    await page.click("#strip [data-add=slide]");
    await changed(before);
    const states = await page.evaluate(() => [...document.querySelectorAll("#strip li")].map((li) => li.dataset.state));
    check(states.length === 2, `+ Slide adds a slide: ${states.join(", ")}`);
    await page.locator("#strip li").nth(1).click();
    await page.waitForFunction(() => window.scaena.shown() === 1, null, { timeout: 30000 });
  });

  let body;
  await step("insert-list", async () => {
    await page.keyboard.press("Escape");
    const offered = await page.evaluate(() => [...document.querySelectorAll("#look-layout option")].map((o) => o.value));
    if (offered.includes("full")) {
      const before = await source();
      await page.selectOption("#look-layout", "full");
      await changed(before).catch(() => {});
    }
    const before = await shownNodes();
    await page.selectOption("#insert", { label: "body" });
    await page.waitForFunction((n) => window.scaena.canvas.boxes().length > n, before.length, { timeout: 60000 });
    body = (await shownNodes()).find((n) => !before.includes(n));
    await typeInto(body, ["Three trails surveyed", "Two need new bridges", "One closes for winter"]);
    const b = await source();
    await page.keyboard.press("Control+a");
    await page.keyboard.press("Control+Shift+Digit8");
    await changed(b);
    await page.keyboard.press("Escape");
    check(/list/.test(await source()), "⌘⇧8 makes the three lines a bulleted list");
  });

  await step("data-on-canvas", async () => {
    // A person drags a CSV onto the slide.
    const [x, y] = await client([1400, 700]);
    const before = await source();
    await dropFile("#overlay", [x, y], "visits.csv", "text/csv", Buffer.from(csv));
    await page.waitForTimeout(1500);
    check((await source()) !== before, `a CSV dropped on the slide joins the deck: ${await status()}`);
  });

  await step("data-tab", async () => {
    await page.click("#tab-data");
    await page.waitForTimeout(500);
    const said = await page.evaluate(() => document.querySelector("#data").textContent.slice(0, 200));
    check(!said.includes("has no data source"), `the Data tab shows the CSV: ${said}`);
    await page.click("#tab-inspector");
  });

  await step("chart", async () => {
    const offered = await page.evaluate(() => [...document.querySelectorAll("#insert optgroup")].map((g) => `${g.label}: ${[...g.children].map((o) => o.textContent).join(", ")}`));
    const chart = await page.evaluate(() => [...document.querySelectorAll("#insert option")].map((o) => o.textContent).find((t) => /chart/i.test(t)));
    if (!check(Boolean(chart), `Insert offers a chart of the CSV: ${offered.join(" / ")}`)) return;
    const before = await shownNodes();
    await page.selectOption("#insert", { label: chart });
    await page.waitForFunction((n) => window.scaena.canvas.boxes().length > n, before.length, { timeout: 60000 });
  });

  await step("photo-on-canvas", async () => {
    const [x, y] = await client([1500, 300]);
    const before = await shownNodes();
    await dropFile("#overlay", [x, y], "trailhead.jpg", "image/jpeg", photo);
    await page.waitForTimeout(3000);
    const after = await shownNodes();
    check(after.length > before.length, `a photo dropped on the slide is put on it: ${await status()}`);
  });

  await step("motion", async () => {
    await page.locator("#strip li").nth(0).click();
    await page.waitForFunction(() => window.scaena.shown() === 0, null, { timeout: 30000 });
    const [x, y] = await middleOf(title);
    await page.mouse.click(x, y);
    await page.waitForFunction(() => document.querySelectorAll("#cue [data-add] option").length > 1, null, { timeout: 30000 });
    const options = await page.evaluate(() => [...document.querySelectorAll("#cue [data-add] option")].map((o) => [o.value, o.textContent]));
    const enter = options.find(([v]) => v.includes('"enter"'));
    if (!check(Boolean(enter), `the cue offers a motion for the title: ${options.map((o) => o[1]).join(", ")}`)) return;
    const before = await source();
    await page.locator("#cue [data-add]").selectOption(enter[0]);
    await changed(before);
    check(true, `the title gets a motion: ${await status()}`);
  });

  await step("findings", async () => {
    const found = await page.evaluate(() => window.scaena.last().findings.map((f) => `${f.code} ${f.state ?? ""} ${f.node ?? ""}`.trim()));
    console.log(`     lint: ${found.join("; ") || "nothing"}`);
    check(!(await page.evaluate(() => window.scaena.last().findings.some((f) => f.severity === "error"))), `lint finds no error: ${found.join("; ")}`);
  });

  await step("save", async () => {
    await page.click("#save");
    await page.waitForFunction(() => window.scaena.where().where && !window.scaena.where().dirty, null, { timeout: 60000 });
    check((await page.evaluate(() => window.scaena.where().where?.kind)) === "opfs", "Save keeps it in the browser");
  });

  await step("play", async () => {
    const [player] = await Promise.all([context.waitForEvent("page", { timeout: 30000 }), page.click("#play")]);
    player.on("pageerror", (e) => failures.push(`player: ${e.message}`));
    await player.waitForFunction(() => document.querySelector("#status")?.textContent.length > 0, null, { timeout: 120000 });
    await player.waitForTimeout(3000);
    await player.screenshot({ path: join(out, `${String(++shot).padStart(2, "0")}-player-first.png`) });
    await player.keyboard.press("ArrowRight");
    await player.waitForTimeout(3000);
    await player.screenshot({ path: join(out, `${String(++shot).padStart(2, "0")}-player-second.png`) });
    check(true, `the player plays it: ${await player.evaluate(() => document.querySelector("#status").textContent)}`);
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
    check(pdf.name.endsWith(".pdf") && pdf.bytes.subarray(0, 5).toString() === "%PDF-", `Export… downloads ${pdf.name}, ${pdf.bytes.length} B`);
  });
  await step("export-html", async () => {
    await page.click("#export");
    await page.check("#exporting input[name=as][value=html]");
    const html = await downloaded(() => page.click("#exporting-go"));
    check(html.name.endsWith(".html") && html.bytes.length > 100000, `Export… downloads ${html.name}, ${html.bytes.length} B`);
  });
  let zip;
  await step("download", async () => {
    zip = await downloaded(() => page.click("#download"));
    check(zip.name.endsWith(".scaena"), `Download gives ${zip.name}, ${zip.bytes.length} B`);
  });
  await step("reopen", async () => {
    const kept = await source();
    await page.goto(editor);
    await page.waitForFunction(() => window.scaena?.last(), null, { timeout: 120000 });
    const [chooser] = await Promise.all([page.waitForEvent("filechooser"), page.click("#open-file")]);
    await chooser.setFiles({ name: zip.name, mimeType: "application/zip", buffer: zip.bytes });
    await ready(zip.name.replace(/\.scaena$/, ""));
    check((await source()) === kept, "the .scaena opens again as it was");
  });
} catch (e) {
  failures.push(String(e));
} finally {
  await browser.close();
  server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "a first deck, made, played, exported, and opened again");
process.exit(failures.length ? 1 : 0);

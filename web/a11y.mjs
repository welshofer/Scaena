// PLAN 2.8 accessibility check: the web player, its presenter view, the source editor, and a
// single file, in headless Chromium (serve.mjs). The CPU paints, as in web/player.mjs.
//
//   node web/a11y.mjs     (after `just web`; from the repository's root)
//
// - axe-core finds nothing against WCAG 2.1 A and AA on any of them, the editor with each tab.
// - For a screen reader: the canvas is hidden, and the live region holds how the state shown
//   reads, the same HTML a single-file export carries for it. What reads as it did stays put
//   as the deck goes on. The state picker groups the states by the spine's sections, each
//   named with its beat's claim.
// - Less motion (the system's preference, or `?motion=reduce`): going on cuts to the next
//   state at rest, with no frame inside its cue, and a state that holds goes on when its cue
//   and hold are over, as it would with motion. `?motion=full` plays the cues anyway.
// - Keys: after a click on ▶ the arrow keys still go on, and Enter on it goes on once; the
//   scrubber says which state it is at. In the editor, a finding is a button that goes to its
//   line, F8 goes to the next one, and the arrow keys move between the tabs; Tab reaches Open
//   .scaena…, and Enter on it opens the file picker.
// Exits 1 on any failure.
import { execFileSync } from "node:child_process";
import { mkdir, readFile } from "node:fs/promises";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { axeInto, launch, serve } from "./serve.mjs";

const server = await serve();
const revenue = "/docs/examples/revenue.deck.json";
const player = (bundle, query = "") => `${server.origin}/web/dist/?painter=cpu&bundle=${bundle}${query}`;
const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};

// How each of the revenue example's states reads, as a single file carries it.
const out = join(process.cwd(), "target/a11y");
await mkdir(out, { recursive: true });
const file = join(out, "revenue.html");
execFileSync("cargo", ["run", "-q", "-p", "scaena-cli", "--locked", "--", "export", revenue.slice(1), "--format", "html", "--out", file], {
  stdio: "inherit",
});
const exported = await readFile(file, "utf8");
const readings = Object.fromEntries([...exported.matchAll(/<template data-state="([^"]+)">(.*?)<\/template>/gs)].map((m) => [m[1], m[2]]));

/** What axe-core finds against WCAG 2.1 A and AA on `page`, each rule with the nodes it names. */
async function audit(page, what) {
  await axeInto(page);
  const found = await page.evaluate(async () => {
    const result = await window.axe.run(document, { runOnly: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"] });
    return result.violations.map((v) => `${v.id} (${v.impact}): ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
  });
  check(!found.length, `${what}: axe finds nothing against WCAG 2.1 AA${found.length ? `:\n    ${found.join("\n    ")}` : ""}`);
}

const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1 });
  const open = async (address, ready = () => window.scaena?.at, into = context) => {
    const page = await into.newPage();
    page.on("pageerror", (e) => failures.push(`${address}: ${e.message}`));
    page.on("console", (m) => m.type() === "error" && failures.push(`${address}: console: ${m.text()}`));
    await page.goto(address);
    await page.waitForFunction(ready, null, { timeout: 120000 });
    return page;
  };
  const at = (page) => page.evaluate(() => window.scaena.at());

  // The player: how the state shown reads, and the outline.
  const page = await open(player(revenue));
  await audit(page, "the player");
  check(await page.$eval("#stage", (c) => c.getAttribute("aria-hidden") === "true"), "the canvas is hidden from a screen reader");
  /** Whether the live region holds `html`, as the browser writes it. */
  const reads = (html) =>
    page
      .waitForFunction(
        (html) => {
          const t = document.createElement("template");
          t.innerHTML = html;
          return document.querySelector("#reading").innerHTML === t.innerHTML;
        },
        html,
        { timeout: 15000 },
      )
      .then(() => true)
      .catch(() => false);
  const live = await page.$eval("#reading", (r) => [r.getAttribute("aria-live"), r.getAttribute("aria-label")]);
  check(live[0] === "polite" && Boolean(live[1]), `the reading is a polite live region, named (${live[1]})`);
  check(await reads(readings.intro), `intro reads as the single file reads it: ${readings.intro}`);
  await page.evaluate(() => window.scaena.seek(1));
  check(await reads(readings.revenue), "revenue reads as the single file reads it");
  await page.evaluate(() => {
    for (const el of document.querySelectorAll("#reading > *")) el.kept = true;
  });
  await page.evaluate(() => window.scaena.seek(2));
  check(await reads(readings.mix), "mix reads as the single file reads it");
  const kept = await page.evaluate(() => [...document.querySelectorAll("#reading > *")].map((el) => [el.dataset.node, Boolean(el.kept)]));
  check(
    JSON.stringify(kept) === JSON.stringify([["title", false], ["rev", true], ["note", true]]),
    `what reads as it did stays put, and the new title is new: ${JSON.stringify(kept)}`,
  );
  const deck = JSON.parse(await readFile(revenue.slice(1), "utf8"));
  const outline = await page.$$eval("#state optgroup", (groups) => groups.map((g) => [g.label, [...g.children].map((o) => o.textContent)]));
  // Each state as a person counts it (PLAN 2.99), with its beat's claim.
  const said = Object.fromEntries((await page.evaluate(() => window.scaena.timeline())).map((s) => [s.state, s.said]));
  const expected = deck.spine.sections.map((s) => [s.title, s.beats.flatMap((b) => b.states.map((state) => `${said[state]} · ${b.claim}`))]);
  check(JSON.stringify(outline) === JSON.stringify(expected), `the state picker is the spine's outline: ${JSON.stringify(outline)}`);
  await page.selectOption("#state", { label: `${said.close} · ${deck.spine.sections.at(-1).beats.at(-1).claim}` });
  await page.waitForFunction(() => window.scaena.at().index === 3 && !window.scaena.at().playing, null, { timeout: 15000 }).catch(() => {});
  check((await at(page)).index === 3, "picking a state from the outline plays it");
  await page.close();

  // The presenter view.
  const presenter = await open(player(revenue, "&view=presenter"), () => document.querySelector("#clock"));
  await presenter.waitForFunction(() => document.querySelector("#status")?.textContent !== "Loading…", null, { timeout: 60000 }).catch(() => {});
  await audit(presenter, "the presenter view");
  await presenter.close();

  // Less motion: the system's preference.
  /** Every place the deck stood at, a sample each frame, from now until `done` holds. */
  const watch = async (page, press, done, timeout) => {
    await page.evaluate(() => {
      window.seen = [];
      const tick = () => {
        window.seen.push({ ...window.scaena.at(), now: performance.now() });
        requestAnimationFrame(tick);
      };
      requestAnimationFrame(tick);
    });
    await press();
    await page.waitForFunction(done, null, { timeout }).catch(() => {});
    return page.evaluate(() => window.seen);
  };
  const still = await browser.newContext({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1, reducedMotion: "reduce" });
  const calm = await open(player(revenue), undefined, still);
  const slots = await calm.evaluate(() => window.scaena.timeline());
  const [, cue, after] = slots;
  const seen = await watch(
    calm,
    () => calm.keyboard.press("ArrowRight"),
    () => window.scaena.at().index === 2,
    cue.span + cue.hold + 15000,
  );
  const inCue = seen.filter((s) => s.index > 0 && s.t < slots[s.index].span);
  check(!inCue.length && seen.some((s) => s.index === 1), `with less motion, going on cuts to ${cue.state} at rest: no frame inside a cue (${inCue.length} were)`);
  const arrived = seen.find((s) => s.index === 1)?.now;
  const left = seen.find((s) => s.index === 2)?.now;
  const stayed = left - arrived;
  check(
    stayed >= cue.span + cue.hold - 50 && stayed < cue.span + cue.hold + 1500,
    `and it keeps the deck's pace: ${cue.state} stays ${Math.round(stayed)} ms, its cue and hold ${cue.span + cue.hold} ms, then ${after.state}`,
  );
  await calm.close();
  const anyway = await open(player(revenue, "&motion=full"), undefined, still);
  const moving = await watch(
    anyway,
    () => anyway.keyboard.press("ArrowRight"),
    () => false,
    cue.span + 500,
  );
  check(moving.some((s) => s.index === 1 && s.t < cue.span), "?motion=full plays the cues anyway");
  await anyway.close();
  await still.close();
  const asked = await open(player(revenue, "&motion=reduce"));
  const cut = await watch(asked, () => asked.keyboard.press("ArrowRight"), () => window.scaena.at().index === 1, 15000);
  check(cut.filter((s) => s.index === 1).every((s) => s.t === cue.span), "?motion=reduce cuts without the system's preference");
  await asked.close();

  // Keys, on the torture deck, whose states do not hold, with less motion so each step is one.
  const keys = await open(player("/tests/fixtures/torture.scaena", "&motion=reduce"));
  const states = await keys.evaluate(() => window.scaena.states);
  await keys.click("#on");
  await keys.waitForFunction(() => window.scaena.at().index === 1, null, { timeout: 15000 }).catch(() => {});
  await keys.keyboard.press("ArrowRight");
  await keys.waitForFunction(() => window.scaena.at().index === 2, null, { timeout: 15000 }).catch(() => {});
  check((await at(keys)).index === 2, "after a click on ▶, → still goes on");
  await keys.keyboard.press("Enter");
  await keys.waitForTimeout(500);
  check((await at(keys)).index === 3, `Enter on ▶ goes on once: ${(await at(keys)).index}`);
  const valuetext = await keys.$eval("#scrub", (s) => s.getAttribute("aria-valuetext"));
  const fourth = (await keys.evaluate(() => window.scaena.timeline()))[3].said;
  check(valuetext === fourth, `the scrubber says where it is, as a person counts the slides: ${valuetext}`);
  await keys.close();

  // The editor.
  const editor = await open(`${server.origin}/web/dist/editor.html?painter=cpu&source=open&bundle=${revenue}`, () => window.scaena?.last());
  await editor.waitForFunction(() => window.scaena.wholes().length > 0, null, { timeout: 60000 }).catch(() => {});
  // The slide's backgrounds, a listbox of swatches (PLAN 2.148), drawn.
  await editor.waitForSelector("#look .backgrounds [role=option]", { timeout: 60000 }).catch(() => {});
  check((await editor.locator("#look .backgrounds [role=option]").count()) > 1, "the inspector offers the slide's backgrounds");
  await audit(editor, "the editor, its inspector");
  // The node selected, its look chosen from the theme (PLAN 2.33).
  await editor.evaluate(() => window.scaena.canvas.select("title"));
  await editor.waitForSelector("#look .fields", { timeout: 30000 }).catch(() => {});
  check((await editor.locator("#look select").count()) > 0, "the inspector shows the title's look to choose");
  await audit(editor, "the editor, a node's look in its inspector");
  // Open .scaena… is reached by the keys: Enter on File opens its menu (lift-up ATTR-5), ↓ moves
  // to it, and Enter on it opens the file picker. The wait for the picker starts before the first
  // key: Playwright only catches a file chooser once its waiting has turned interception on, which
  // it does asynchronously, and a picker opened before that passes it by.
  const chooser = editor.waitForEvent("filechooser", { timeout: 5000 }).catch(() => null);
  await editor.focus("#file-open");
  await editor.keyboard.press("Enter");
  let reached = await editor.evaluate(() => document.activeElement?.id);
  for (let i = 0; i < 6 && reached !== "open-scaena" && reached !== "download"; i++) {
    await editor.keyboard.press("ArrowDown");
    reached = await editor.evaluate(() => document.activeElement?.id);
  }
  if (reached === "open-scaena") await editor.keyboard.press("Enter");
  check(reached === "open-scaena" && (await chooser) !== null, `Open .scaena… is reached from File by the keys, and Enter opens the file picker: ${reached}`);
  await editor.keyboard.press("Escape");
  const source = await editor.evaluate(() => window.scaena.source());
  const long = "Revenue doubled, and then some, as Pro grew three times faster than Core in every quarter";
  await editor.evaluate((t) => window.scaena.type(t), source.replace('"Revenue doubled"', `"${long}"`));
  await editor.waitForFunction(() => document.querySelector("#problems li button.said"), null, { timeout: 30000 }).catch(() => {});
  const line = String((await editor.evaluate(() => window.scaena.source())).split("\n").findIndex((l) => l.includes(long)) + 1);
  const gutter = () => editor.evaluate(() => document.querySelector(".cm-activeLineGutter")?.textContent);
  await editor.evaluate(() => window.scaena.cursor(0));
  await editor.focus("#problems li button.said");
  await editor.keyboard.press("Enter");
  check(
    (await gutter()) === line && (await editor.evaluate(() => Boolean(document.activeElement?.closest(".cm-editor")))),
    `Enter on a finding goes to its line (${line}) in the source: ${await gutter()}`,
  );
  await editor.evaluate(() => window.scaena.cursor(0));
  await editor.focus(".cm-content");
  await editor.keyboard.press("F8");
  check((await gutter()) === line, `F8 goes to the next finding: line ${await gutter()}`);
  // The inspector's tabs on one row (lift-up ATTR-4, PLAN 3.18): Format, Animate, Document, and
  // More, a menu of the layers and the assistant; Document's panels a row of their own. The arrow
  // keys move along each row, and only tabs a person sees are in the ring.
  const tab = (panel) =>
    editor.evaluate(
      (p) => [
        document.activeElement?.id,
        [...document.querySelectorAll('[role="tab"][aria-selected="true"]')].filter((t) => !t.closest("[hidden]")).map((t) => t.id).join(" "),
        document.querySelector(p).hidden,
      ],
      panel,
    );
  const rowOf = (list) =>
    editor.evaluate((l) => new Set([...document.querySelectorAll(`${l} [role="tab"]:not([hidden])`)].map((b) => b.offsetTop)).size, list);
  // At its narrowest: the source folded, in a window 1000 px wide, the panel is 280 px.
  const widths = await editor.evaluate(() => [innerWidth, innerHeight]);
  await editor.click("#source-toggle");
  await editor.setViewportSize({ width: 1000, height: widths[1] });
  const narrow = await editor.evaluate(() => document.querySelector("#panel").getBoundingClientRect().width);
  check((await rowOf("#tabs")) === 1, `the tabs stand on one row at the panel's narrowest (${narrow} px): ${await rowOf("#tabs")} row(s)`);
  await editor.setViewportSize({ width: widths[0], height: widths[1] });
  await editor.click("#source-toggle");
  await editor.focus("#tab-inspector");
  await editor.keyboard.press("ArrowRight");
  check(JSON.stringify(await tab("#animate")) === JSON.stringify(["tab-animate", "tab-animate", false]), `→ on the tabs shows Animate: ${JSON.stringify(await tab("#animate"))}`);
  await audit(editor, "the editor, Animate");
  await editor.keyboard.press("ArrowRight");
  check(
    JSON.stringify(await tab("#document")) === JSON.stringify(["tab-document", "tab-document tab-theming", false]),
    `→ again shows Document, at its theme: ${JSON.stringify(await tab("#document"))}`,
  );
  check((await rowOf("#document .segments")) === 1, `Document's panels stand on one row: ${await rowOf("#document .segments")} row(s)`);
  await editor.waitForFunction(() => window.scaena.theme.theme() !== undefined, null, { timeout: 60000 }).catch(() => {});
  await audit(editor, "the editor, its theme");
  await editor.focus("#tab-theming");
  await editor.keyboard.press("ArrowRight");
  check(JSON.stringify(await tab("#data")) === JSON.stringify(["tab-data", "tab-document tab-data", false]), `→ along Document's row shows the data: ${JSON.stringify(await tab("#data"))}`);
  await editor.waitForFunction(() => window.scaena.data.shown()?.sheet.rows.length > 0, null, { timeout: 60000 }).catch(() => {});
  await audit(editor, "the editor, its data");
  await editor.keyboard.press("ArrowRight");
  check(JSON.stringify(await tab("#files")) === JSON.stringify(["tab-files", "tab-document tab-files", false]), `→ again shows the bundle's files: ${JSON.stringify(await tab("#files"))}`);
  await editor.waitForFunction(() => window.scaena.files.listed().length > 0, null, { timeout: 60000 }).catch(() => {});
  await audit(editor, "the editor, its files");
  await editor.keyboard.press("ArrowRight");
  check(JSON.stringify(await tab("#versions")) === JSON.stringify(["tab-versions", "tab-document tab-versions", false]), `→ again shows the deck's versions: ${JSON.stringify(await tab("#versions"))}`);
  await editor.waitForFunction(() => window.scaena.versions.listed() === null, null, { timeout: 60000 }).catch(() => {});
  await audit(editor, "the editor, its versions");
  await editor.keyboard.press("ArrowRight");
  check(JSON.stringify(await tab("#story")) === JSON.stringify(["tab-story", "tab-document tab-story", false]), `→ again shows the deck's story: ${JSON.stringify(await tab("#story"))}`);
  await editor.waitForFunction(() => window.scaena.story.health() !== undefined, null, { timeout: 60000 }).catch(() => {});
  check(await editor.evaluate(() => window.scaena.story.health() !== undefined), "the deck's story is read");
  await audit(editor, "the editor, its story");
  await editor.keyboard.press("ArrowRight");
  check((await tab("#theming"))[0] === "tab-theming", "→ again comes round to the theme");
  // More: ↓ opens its menu, whose items are the panels it keeps; one chosen shows as More's own tab.
  await editor.focus("#tab-document");
  await editor.keyboard.press("ArrowRight");
  check((await tab("#document"))[0] === "tab-more", `→ from Document goes to More, which stands for no panel yet: ${JSON.stringify(await tab("#document"))}`);
  await editor.keyboard.press("ArrowDown");
  const items = await editor.evaluate(() => [...document.querySelectorAll('.context-menu [role="menuitem"]')].map((b) => b.textContent));
  check(JSON.stringify(items) === JSON.stringify(["Layers", "Assistant"]), `↓ on More opens its menu: ${JSON.stringify(items)}`);
  await audit(editor, "the editor, More's menu");
  await editor.keyboard.press("Enter");
  check(JSON.stringify(await tab("#layers")) === JSON.stringify(["tab-more", "tab-more", false]), `Enter on Layers shows the layers as More's own tab: ${JSON.stringify(await tab("#layers"))}`);
  check((await editor.textContent("#tab-more")) === "Layers", `named for it: ${await editor.textContent("#tab-more")}`);
  await audit(editor, "the editor, its layers");
  await editor.keyboard.press("ArrowDown");
  await editor.keyboard.press("ArrowDown");
  await editor.keyboard.press("Enter");
  check(JSON.stringify(await tab("#assistant")) === JSON.stringify(["tab-more", "tab-more", false]), `and so the assistant: ${JSON.stringify(await tab("#assistant"))}`);
  await audit(editor, "the editor, its assistant");
  await editor.keyboard.press("ArrowRight");
  check((await tab("#inspector"))[0] === "tab-inspector" && !(await tab("#inspector"))[2], "→ again comes round to Format");
  await editor.close();

  // A single file, from its address on disk.
  const single = await context.newPage();
  await single.goto(`${pathToFileURL(file).href}?painter=cpu`);
  await single.waitForFunction(() => window.scaena?.at, null, { timeout: 120000 });
  await audit(single, "a single file");
  await single.close();
} finally {
  await browser.close();
  server.close();
}

if (failures.length) {
  console.error(`\n${failures.length} failure(s):\n${failures.join("\n")}`);
  process.exit(1);
}
console.log("\nthe player, the presenter view, the editor, and a single file pass the checks for a reader");

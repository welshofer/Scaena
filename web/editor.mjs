// PLAN 2.3 editor check: the source editor in headless Chromium (serve.mjs), on the revenue
// example, then the edit round trip timed on B1, the 40-state benchmark deck. The CPU paints,
// as in web/player.mjs.
//
//   node web/editor.mjs     (after `just web`; from the repository's root)
//
// - The editor opens on the deck's canonical source, and the deck lints clean.
// - An edit that compiles shows at once: a headline too long for its slot is E100, placed on
//   the line that sets it, with a fix in the gutter. The fix rewrites only what it changes,
//   and the finding goes.
// - A source that does not compile says where; the preview keeps the deck it had.
// - The preview and the inspector follow the cursor from state to state.
// - Once typing stops, every state is linted, and what that finds replaces what the edit's
//   lint of the state shown left standing.
// - A node put in one state and taken out by an undo: before every state is linted again, no
//   finding names it, from any state it was in, and the status says what that lint will.
// - On B1, each edit's round trip (compile, the frame, lint of the state shown) is recorded;
//   gate 2 asks for under 200 ms (PLAN §2). So is the lint of every state that follows.
// Exits 1 on any failure.
import { readFile } from "node:fs/promises";
import { launch, serve } from "./serve.mjs";

const server = await serve();
const url = (bundle) => `${server.origin}/web/dist/editor.html?painter=cpu&source=open&bundle=${bundle}`;
const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};

const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1920, height: 1200 }, deviceScaleFactor: 1 });
  const open = async (bundle) => {
    const page = await context.newPage();
    page.on("pageerror", (e) => failures.push(`${bundle}: ${e.message}`));
    page.on("console", (m) => m.type() === "error" && failures.push(`${bundle}: console: ${m.text()}`));
    await page.goto(url(bundle));
    await page.waitForFunction(
      () => window.scaena?.last() || document.querySelector("#status")?.textContent.startsWith("Error"),
      null,
      { timeout: 120000 },
    );
    const status = await page.evaluate(() => (window.scaena?.last() ? "" : document.querySelector("#status").textContent));
    if (status) throw new Error(`${bundle}: ${status}`);
    return page;
  };
  /** The edit after `before` edits: once lint has answered for the source now in the editor. */
  const settled = (page, before) =>
    page.waitForFunction((n) => window.scaena.trips().length > n, before, { timeout: 60000, polling: 50 }).then(() =>
      page.evaluate(() => window.scaena.last()),
    );
  const type = async (page, text) => {
    const before = await page.evaluate(() => window.scaena.trips().length);
    await page.evaluate((t) => window.scaena.type(t), text);
    return settled(page, before);
  };
  const canvas = (page) => page.locator("#stage").screenshot();

  // The revenue example.
  const page = await open("/docs/examples/revenue.deck.json");
  const canonical = await readFile("docs/examples/revenue.deck.scn", "utf8");
  const source = await page.evaluate(() => window.scaena.source());
  check(source === canonical, "the editor opens on the deck's canonical source (docs/examples/revenue.deck.scn)");
  const first = await page.evaluate(() => window.scaena.last());
  check(first.valid && first.findings.every((f) => f.severity !== "error"), "the revenue example lints with no error");

  // An edit: a headline too long for its slot.
  const long = "Revenue doubled, and then some, as Pro grew three times faster than Core in every quarter";
  const edited = source.replace('"Revenue doubled"', `"${long}"`);
  let last = await type(page, edited);
  const overflow = last.findings.find((f) => f.code === "E100" && f.fix);
  const line = edited.split("\n").findIndex((l) => l.includes(long)) + 1;
  check(Boolean(overflow), `the long headline is E100, with a fix: ${overflow?.message ?? JSON.stringify(last.findings.map((f) => f.code))}`);
  check(overflow?.at?.line === line, `E100 stands on the line that sets the headline (${line}): ${overflow?.at?.line}`);
  check((await page.locator(".cm-lint-marker-error").count()) > 0, "the gutter marks it");
  check((await page.evaluate(() => window.scaena.shown())) === 1, "the preview shows the state being edited (`revenue`)");
  check(last.whole === false, "the edit's lint lays out the state shown");
  const wholeBefore = await page.evaluate(() => window.scaena.wholes().length);
  await page.waitForFunction((n) => window.scaena.wholes().length > n, wholeBefore, { timeout: 30000 }).catch(() => {});
  const whole = await page.evaluate(() => window.scaena.last());
  check(whole.whole && whole.findings.some((f) => f.code === "E100" && f.fix), "once typing stops, every state is linted, and the E100 stands");

  // Its fix.
  const before = await page.evaluate(() => window.scaena.trips().length);
  await page.evaluate(() => window.scaena.fix("E100"));
  last = await settled(page, before);
  const fixed = await page.evaluate(() => window.scaena.source());
  check(fixed.includes(long), "the fix keeps the headline's words");
  // The lines the fix writes, wherever they fall: setting it in canonical form may wrap a long line.
  const was = new Set(edited.split("\n"));
  const changed = fixed.split("\n").filter((l) => !was.has(l)).length;
  check(changed > 0 && changed <= 3, `the fix changes only the lines it is about: ${changed}`);
  // 9:16's header is too narrow for words that run past the deck's canvas's: they overflow there
  // at any size the role allows, which only shorter words mend.
  check(!last.findings.some((f) => f.code === "E100" && f.format === undefined), "after the fix, no E100 on the deck's canvas");

  // A source that does not compile, typed in `close`, which the preview shows from the
  // cursor on.
  const inClose = fixed.indexOf("state close") + "state ".length;
  await page.evaluate((at) => window.scaena.cursor(at), inClose);
  await page.waitForFunction(() => window.scaena.at().index === 3, null, { timeout: 15000 }).catch(() => {});
  // The canvas shows `close` once it holds its boxes, a frame after the worker paints it: the worker
  // can still be judging the last state's layouts when the cursor moves.
  await page
    .waitForFunction(() => window.scaena.canvas.boxed() === "close" && window.scaena.canvas.boxes().length > 0, null, { timeout: 30000 })
    .catch(() => {});
  await page.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))));
  const shownBefore = await canvas(page);
  const broken = fixed.replace("state close layout:title", "state close layout:");
  last = await type(page, broken);
  const brokenLine = broken.split("\n").findIndex((l) => l.startsWith("state close")) + 1;
  check(Boolean(last.error) && last.error.at?.line === brokenLine, `a source that does not compile says where: ${JSON.stringify(last.error?.at)} (line ${brokenLine})`);
  check((await canvas(page)).equals(shownBefore), "the preview keeps the deck it had");
  last = await type(page, fixed);
  check(last.valid && !last.error, "putting it right compiles again");

  // The cursor leads the preview and the inspector.
  const close = fixed.indexOf("state close") + "state ".length;
  await page.evaluate((at) => window.scaena.cursor(at), close);
  await page.waitForFunction(() => window.scaena.shown() === 3 && window.scaena.at().index === 3, null, { timeout: 15000 }).catch(() => {});
  check((await page.evaluate(() => window.scaena.at().index)) === 3, "putting the cursor in `close` shows `close`");
  await page.evaluate((at) => window.scaena.cursor(at), fixed.indexOf("state revenue"));
  await page
    .waitForFunction(() => window.scaena.inspector().includes("Slide 2") && window.scaena.inspector().includes("Headline"), null, { timeout: 15000 })
    .catch(() => {});
  const inspector = await page.evaluate(() => window.scaena.inspector());
  check(inspector.includes("Headline") && inspector.includes("transition"), `the inspector shows the title's look and the cue: ${inspector.slice(0, 120)}`);

  // A copy of the chart put in `revenue`, low on the slide, as a paste puts one: `mix` and
  // `close` keep it, and once every state is linted, findings name it there.
  const names = (f) => JSON.stringify(f).includes("rev-2");
  const [from, to] = [fixed.indexOf("  rev chart:"), fixed.indexOf("  note text")];
  const copy = fixed.slice(from, to).replace("  rev ", "  rev-2 ").replace("at:in(main)", "at:{rect: [600, 820, 1100, 600]}");
  const wholesBefore = await page.evaluate(() => window.scaena.wholes().length);
  await type(page, fixed.slice(0, to) + copy + fixed.slice(to));
  await page.waitForFunction((n) => window.scaena.wholes().length > n, wholesBefore, { timeout: 30000 }).catch(() => {});
  const pasted = await page.evaluate(() => window.scaena.last());
  const elsewhere = pasted.findings.filter((f) => names(f) && f.state === "close");
  check(pasted.whole && elsewhere.length > 0, `once every state is linted, findings in \`close\` name the copy: ${elsewhere.map((f) => f.code)}`);
  // Undone in the source, the copy is gone from what the edit's lint answers at once, which
  // keeps nothing from the states it was in until every state is linted again.
  const tripsBefore = await page.evaluate(() => window.scaena.trips().length);
  const wholesThen = await page.evaluate(() => window.scaena.wholes().length);
  await page.focus(".cm-content");
  await page.keyboard.press("Control+z");
  const undone = await page
    .waitForFunction(
      (n) => window.scaena.trips().length > n && { last: window.scaena.last(), status: document.querySelector("#status").textContent, source: window.scaena.source() },
      tripsBefore,
      { timeout: 60000, polling: 50 },
    )
    .then((h) => h.jsonValue());
  check(undone.source === fixed, "one undo takes the copy out");
  const named = undone.last.findings.filter(names);
  check(!undone.last.whole && named.length === 0, `before every state is linted again, no finding names the copy: ${named.map((f) => `${f.code} ${f.state}`)}`);
  await page.waitForFunction((n) => window.scaena.wholes().length > n, wholesThen, { timeout: 30000 }).catch(() => {});
  const counts = (status) => status.split(" · ")[0];
  const settledStatus = await page.evaluate(() => document.querySelector("#status").textContent);
  check(counts(undone.status) === counts(settledStatus), `the status says at once what every state's lint does: "${counts(undone.status)}", then "${counts(settledStatus)}"`);
  await page.close();

  // B1: the round trip, timed.
  const b1 = await open("/tests/bench/b1.scaena");
  const b1source = await b1.evaluate(() => window.scaena.source());
  const word = b1source.match(/"([A-Z][a-z]{4,})/);
  for (let i = 0; i < 6; i++) await type(b1, b1source.replace(word[0], `"${word[1]}${"s".repeat(i + 1)}`));
  const trips = await b1.evaluate(() => window.scaena.trips());
  const sorted = trips.slice(1).map((t) => t.ms).sort((a, b) => a - b);
  const median = sorted[sorted.length >> 1];
  const parts = ["compile", "paint", "lint"].map((k) => `${k} ${trips.at(-1)[k].toFixed(0)}`).join(", ");
  console.log(`B1 edit round trip: median ${median.toFixed(0)} ms over ${sorted.length} edits (last: ${parts} ms); gate 2 asks under 200`);
  check(trips.at(-1) && (await b1.evaluate(() => window.scaena.last().valid)), "B1's edits compile");
  await b1.waitForFunction(() => window.scaena.wholes().length > 0 && window.scaena.last().whole, null, { timeout: 60000 }).catch(() => {});
  const wholes = await b1.evaluate(() => window.scaena.wholes());
  check(wholes.length > 0, "once typing stops, B1's every state is linted");
  if (wholes.length) console.log(`B1 lint of every state, once typing stops: ${wholes.at(-1).ms.toFixed(0)} ms`);

  // The toolbar (lift-up ATTR-5): one row of grouped icon-and-label buttons, every control of the
  // two rows it replaces still there by its id; the File menu under its button, its items the
  // File controls; Escape and a press elsewhere close it; and the arrows move along the bar. On
  // the editor as it opens, the source folded: with the source open beside it, the column is
  // narrower than the labels, which fold away, and the bar takes two rows.
  const tb = await context.newPage();
  tb.on("pageerror", (e) => failures.push(`toolbar: ${e.message}`));
  await tb.goto(`${server.origin}/web/dist/editor.html?painter=cpu&bundle=/tests/bench/b1.scaena`);
  await tb.waitForFunction(() => window.scaena?.last(), null, { timeout: 120000 });
  const bar = await tb.evaluate(() => {
    const bar = document.querySelector("#toolbar");
    const groups = [...bar.querySelectorAll('[role="group"]')].map((g) => g.getAttribute("aria-label"));
    const buttons = [...bar.querySelectorAll("button, a.button")].filter((b) => !b.hidden && !b.closest('[role="menu"]'));
    const dressed = buttons.filter((b) => b.querySelector("svg path") && b.querySelector(".label")?.textContent.trim());
    const ids = ["source-toggle", "new-deck", "new-prompt", "open-scaena", "kept", "save", "save-as", "download", "export", "play", "state", "format", "slides-open", "formats-open", "rehearse-open", "theme", "insert", "find-open", "commands-open", "keys-open", "grid", "safe", "layout-edit", "status"];
    const missing = ids.filter((id) => !document.getElementById(id));
    const rows = new Set(buttons.map((b) => Math.round(b.getBoundingClientRect().top)));
    return { role: bar.getAttribute("role"), groups, buttons: buttons.length, dressed: dressed.length, missing, rows: rows.size, width: innerWidth };
  });
  check(
    bar.role === "toolbar" && bar.groups.join(",") === "File,View,Slide,Zoom,Insert,Play,Find" && bar.missing.length === 0 && bar.buttons === bar.dressed && bar.buttons >= 16,
    `the toolbar's ${bar.buttons} buttons stand in groups (${bar.groups.join(", ")}), each with an icon and a label, every control by its id${bar.missing.length ? `; missing ${bar.missing.join(", ")}` : ""}`,
  );
  check(bar.rows === 1, `on one row at ${bar.width} px wide: ${bar.rows}`);
  // From a prompt…, the AI-first way in, stands on the bar right after File, not in its menu.
  const prompt = await tb.evaluate(() => {
    const along = [...document.querySelectorAll("#toolbar button, #toolbar a.button")].filter((b) => !b.hidden && !b.closest('[role="menu"]'));
    const at = along.findIndex((b) => b.id === "new-prompt");
    return { after: along[at - 1]?.id, menu: !!document.querySelector("#new-prompt").closest('[role="menu"]'), popup: document.querySelector("#new-prompt").getAttribute("aria-haspopup") };
  });
  check(prompt.after === "file-open" && !prompt.menu && prompt.popup === "dialog", `From a prompt… is a button on the bar right after File: ${JSON.stringify(prompt)}`);
  const menuShut = await tb.evaluate(() => document.querySelector("#file-menu").hidden && document.querySelector("#file-open").getAttribute("aria-expanded") === "false");
  await tb.click("#file-open");
  const menuOpen = await tb.evaluate(() => {
    const menu = document.querySelector("#file-menu");
    // Open folder… shows where the browser lets a page open a folder (File System Access).
    const items = [...menu.querySelectorAll("button")].filter((b) => !b.hidden && b.id !== "open-folder").map((b) => b.id);
    const under = menu.getBoundingClientRect().top >= document.querySelector("#file-open").getBoundingClientRect().bottom - 1;
    const folder = !document.querySelector("#open-folder").hidden === "showDirectoryPicker" in window;
    return { hidden: menu.hidden, expanded: document.querySelector("#file-open").getAttribute("aria-expanded"), items, focused: document.activeElement?.id, under, folder };
  });
  check(
    menuShut && !menuOpen.hidden && menuOpen.expanded === "true" && menuOpen.under && menuOpen.folder && menuOpen.focused === "new-deck" && menuOpen.items.join(",") === "new-deck,open-scaena,kept,save-as,download",
    `File opens its menu under the button, its first item focused: ${JSON.stringify(menuOpen)}`,
  );
  await tb.keyboard.press("ArrowDown");
  await tb.keyboard.press("End");
  const ended = await tb.evaluate(() => document.activeElement?.id);
  await tb.keyboard.press("Escape");
  const escaped = await tb.evaluate(() => ({ hidden: document.querySelector("#file-menu").hidden, focused: document.activeElement?.id }));
  check(ended === "download" && escaped.hidden && escaped.focused === "file-open", `the arrows and End move in it, and Escape closes it, the focus back on File: ${ended}, ${JSON.stringify(escaped)}`);
  await tb.click("#file-open");
  await tb.mouse.click(10, 10);
  check(await tb.evaluate(() => document.querySelector("#file-menu").hidden), "a press elsewhere closes it");
  await tb.focus("#save");
  await tb.keyboard.press("ArrowRight");
  const right = await tb.evaluate(() => document.activeElement?.id);
  await tb.keyboard.press("Home");
  const home = await tb.evaluate(() => document.activeElement?.id);
  check(right === "export" && home === "file-open", `→ moves along the toolbar and Home to its start: ${right}, ${home}`);
  await tb.close();
} catch (e) {
  failures.push(String(e));
} finally {
  await browser.close();
  server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "the editor compiles, lints, fixes, previews, and inspects");
process.exit(failures.length ? 1 : 0);

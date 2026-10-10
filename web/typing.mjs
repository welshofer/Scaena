// PLAN 2.32 typing check: text typed where it stands on the editor's canvas, in headless Chromium
// (serve.mjs), the CPU painting, as in web/canvas.mjs.
//
//   node web/typing.mjs     (after `just web`; from the repository's root)
//
// On the revenue example:
// - A double click on the title in `revenue` types in it, and the status says where what is typed
//   goes: in this state, which sets the title's text.
// - Typed keys change the source, a `replace_text` each; the caret the engine draws moves on.
// - A burst of typing is one step to undo, from the canvas, and one to redo.
// - Shift with Home selects to the line's start, by the engine's line, and typing replaces the
//   selection; an input method's composition goes in as it is composed.
// - The note reads its own text in `revenue` and `mix`: typing there says "in 2 states"; with Alt,
//   it is kept to `revenue`, and the node keeps its text.
// - Escape stops typing, the node still selected; keys typed straight before it all go in, one
//   step to undo.
// On the torture deck: up, down, home, and end go by the engine's lines in the `pretty` case's
// paragraph; and in the `mixed` case, a text set by runs keeps them as it is typed in.
// Exits 1 on any failure.
import { launch, serve } from "./serve.mjs";

const server = await serve();
const url = (bundle) => `${server.origin}/web/dist/editor.html?painter=cpu&bundle=${bundle}`;
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
      () => window.scaena?.last() || document.querySelector("#status")?.textContent.startsWith("error"),
      null,
      { timeout: 120000 },
    );
    const status = await page.evaluate(() => (window.scaena?.last() ? "" : document.querySelector("#status").textContent));
    if (status) throw new Error(`${bundle}: ${status}`);
    return page;
  };
  const source = (page) => page.evaluate(() => window.scaena.source());
  const status = (page) => page.evaluate(() => document.querySelector("#status").textContent);
  const box = (page, node) => page.evaluate((n) => window.scaena.canvas.boxes().find((b) => b.node === n)?.rect, node);
  const typed = (page) => page.evaluate(() => window.scaena.canvas.typed());
  const center = ([x, y, w, h]) => [x + w / 2, y + h / 2];
  const client = (page, [x, y]) =>
    page.evaluate(
      ([x, y]) => {
        const r = document.querySelector("#overlay").getBoundingClientRect();
        const [w, h] = window.scaena.canvas.size();
        return [r.left + (x / w) * r.width, r.top + (y / h) * r.height];
      },
      [x, y],
    );
  /** The caret's left edge as drawn, canvas units; none while a selection is drawn. */
  const caretX = (page) =>
    page.evaluate(() => {
      const caret = document.querySelector("#overlay rect.caret");
      return caret ? Number(caret.getAttribute("x")) : undefined;
    });
  /** Once the source holds `text`, the worker is done with what was typed, and lint has answered. */
  const reads = async (page, test) => {
    await page.waitForFunction(
      (t) => window.scaena.source().includes(t) && !window.scaena.canvas.typed()?.sending,
      test,
      { timeout: 30000, polling: 50 },
    );
    await page.waitForTimeout(250);
  };
  const back = async (page, to) => {
    await page.waitForFunction((s) => window.scaena.source() === s, to, { timeout: 30000, polling: 50 }).catch(() => {});
    return (await source(page)) === to;
  };
  const showState = async (page, state, index) => {
    const at = (await source(page)).indexOf(`state ${state}`) + "state ".length;
    await page.evaluate((offset) => window.scaena.cursor(offset), at);
    await page.waitForFunction((i) => window.scaena.shown() === i && window.scaena.at().index === i, index, { timeout: 30000 });
    // The canvas's boxes are the shown state's, not the one shown before it.
    await page.waitForFunction((s) => window.scaena.canvas.boxed() === s && window.scaena.canvas.boxes().length > 0, state, { timeout: 30000 });
  };
  /** Double click `node` at its box's middle, with `keys` held, and wait until it is typed in. */
  const typeIn = async (page, node, keys = []) => {
    await page.waitForFunction((n) => window.scaena.canvas.boxes().some((b) => b.node === n), node, { timeout: 30000 });
    const [x, y] = await client(page, center(await box(page, node)));
    for (const key of keys) await page.keyboard.down(key);
    await page.mouse.dblclick(x, y);
    for (const key of keys) await page.keyboard.up(key);
    await page.waitForFunction((n) => window.scaena.canvas.typing() === n, node, { timeout: 30000 }).catch(() => {});
    await page.waitForFunction(() => document.querySelector("#status").textContent.startsWith("typing in"), null, { timeout: 30000 }).catch(() => {});
    return page.evaluate(() => window.scaena.canvas.typing());
  };

  const page = await open("/docs/examples/revenue.deck.json");
  await showState(page, "revenue", 1);
  const original = await source(page);

  // A double click on the title types in it, where its text lives: `revenue` sets it.
  check((await typeIn(page, "title")) === "title", "a double click on the title types in it");
  const where = await status(page);
  check(where.includes("typing in title") && where.includes("in this state"), `the status says where typing goes: ${where}`);
  check((await page.evaluate(() => document.activeElement?.classList.contains("typing"))), "the keys go to what is typed in");
  check((await page.locator("#overlay rect.caret").count()) === 1, "the canvas draws the caret");
  check((await typed(page))?.value === "Revenue doubled", `what is typed in is the title as \`revenue\` shows it: ${(await typed(page))?.value}`);

  // Typing at the end: the source takes it, and the caret the engine draws moves on.
  await page.keyboard.press("End");
  const before = await caretX(page);
  const started = Date.now();
  await page.keyboard.type("!!");
  await reads(page, "Revenue doubled!!");
  const once = await source(page);
  check(once.includes('"Revenue doubled!!"') && !once.includes('"Q3 Review!!"'), "typing changes the title where `revenue` sets it");
  const after = await caretX(page);
  check(before !== undefined && after !== undefined && after > before, `the caret moves on as it is typed: ${before} → ${after}`);
  console.log(`     two keys typed and laid out again in ${Date.now() - started} ms`);

  // How long a key takes: from the key to the source that holds it, the state shown painted and
  // linted, one key at a time.
  const times = [];
  let line = "Revenue doubled!!";
  for (const key of "abcdefgh") {
    line += key;
    const sent = Date.now();
    await page.keyboard.type(key);
    await page.waitForFunction(
      (t) => window.scaena.source().includes(`"${t}"`) && !window.scaena.canvas.typed()?.sending,
      line,
      { timeout: 30000, polling: 10 },
    );
    times.push(Date.now() - sent);
  }
  times.sort((a, b) => a - b);
  console.log(`     a key typed, made, painted, and linted: median ${times[times.length >> 1]} ms, worst ${times.at(-1)} ms`);
  for (let i = 0; i < 8; i++) await page.keyboard.press("Backspace");
  await reads(page, '"Revenue doubled!!"');

  // One burst, one undo; and one redo.
  await page.keyboard.press("Control+z");
  check(await back(page, original), "one undo takes the burst of typing back");
  await page.waitForFunction(() => window.scaena.canvas.typed()?.value === "Revenue doubled", null, { timeout: 30000 }).catch(() => {});
  check((await typed(page))?.value === "Revenue doubled", "and what is typed in follows the source");
  await page.keyboard.press("Control+Shift+z");
  check(await back(page, once), "one redo types it again");
  await page.keyboard.press("Control+z");
  check(await back(page, original), "and undo again");
  await page.waitForFunction(() => window.scaena.canvas.typed()?.value === "Revenue doubled", null, { timeout: 30000 }).catch(() => {});

  // A key typed as an undo is made goes where the undo leaves the caret, and is a step to undo of
  // its own, not one with the burst before the one undone.
  await page.keyboard.press("End");
  await page.keyboard.type("1");
  await reads(page, '"Revenue doubled1"');
  const one = await source(page);
  await page.waitForTimeout(1100);
  await page.keyboard.type("2");
  await reads(page, '"Revenue doubled12"');
  await page.keyboard.press("Control+z");
  await page.keyboard.type("q");
  await reads(page, '"Revenue doubled1q"');
  check((await source(page)).includes('"Revenue doubled1q"'), "a key typed as an undo is made goes where the undo leaves the caret");
  await page.keyboard.press("Control+z");
  check(await back(page, one), "and is a step to undo of its own");
  await page.keyboard.press("Control+z");
  check(await back(page, original), "the burst before it, another");
  await page.waitForFunction(() => window.scaena.canvas.typed()?.value === "Revenue doubled", null, { timeout: 30000 }).catch(() => {});

  // Shift with Home selects to the line's start; typing replaces the selection.
  await page.keyboard.press("End");
  await page.keyboard.press("Shift+Home");
  const selected = await typed(page);
  check(selected?.from === 0 && selected?.to === "Revenue doubled".length, `Shift and Home select the line: ${JSON.stringify(selected)}`);
  check((await page.locator("#overlay rect.text-selection").count()) >= 1, "the canvas draws the selection");
  await page.keyboard.type("Sales grew");
  await reads(page, '"Sales grew"');
  check((await source(page)).includes('"Sales grew"'), "typing replaces the selection");

  // An input method's composition goes in as it is composed, and its commit stays.
  const cdp = await context.newCDPSession(page);
  await page.keyboard.press("End");
  await cdp.send("Input.imeSetComposition", { text: "´", selectionStart: 1, selectionEnd: 1 });
  await cdp.send("Input.insertText", { text: "é" });
  await reads(page, '"Sales grewé"');
  check((await source(page)).includes('"Sales grewé"'), "a composed character goes in");
  await page.keyboard.press("Escape");
  check((await page.evaluate(() => window.scaena.canvas.typing())) === undefined, "Escape stops typing");
  check((await page.evaluate(() => window.scaena.canvas.selected())) === "title", "and the title stays selected");
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+z");
  check(await back(page, original), "one undo takes back the replacement and what was composed after it");

  // Keys typed straight before Escape all go in, though the change before them is still being
  // made: leaving typing loses none of them, and one undo takes the burst back.
  check((await typeIn(page, "title")) === "title", "a double click types in the title again");
  await page.keyboard.press("End");
  await page.keyboard.type(" and then some");
  await page.keyboard.press("Escape");
  await page.waitForFunction(() => window.scaena.source().includes('"Revenue doubled and then some"'), null, { timeout: 30000 }).catch(() => {});
  check((await source(page)).includes('"Revenue doubled and then some"'), "keys typed straight before Escape all go in");
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+z");
  check(await back(page, original), "one undo takes the burst back");

  // The note reads its own text in `revenue` and in `mix`, which tracks it.
  check((await typeIn(page, "note")) === "note", "a double click on the note types in it");
  const noted = await status(page);
  check(noted.includes("in 2 states"), `the note reads its own text in \`revenue\` and \`mix\`: ${noted}`);
  await page.keyboard.press("Escape");

  // With Alt, typing is kept to the state shown: the node keeps its text, and `revenue` reads the
  // note as typed.
  check((await typeIn(page, "note", ["Alt"])) === "note", "Alt and a double click type in the note");
  const kept = await status(page);
  check(kept.includes("kept to revenue"), `with Alt, typing is kept to the state shown: ${kept}`);
  await page.keyboard.press("Control+Home");
  await page.keyboard.type("Net ");
  await reads(page, '"Net Revenue in $M.');
  const forked = await source(page);
  check(forked.includes('"Revenue in $M. Enterprise recognized on delivery."'), "the node keeps its own text");
  check(forked.includes('"Net Revenue in $M. Enterprise recognized on delivery."'), "and `revenue` reads it as typed");
  await page.keyboard.press("Control+z");
  check(await back(page, original), "one undo takes it back");
  check((await page.evaluate(() => window.scaena.last().valid)), "the source still compiles and validates");

  // A paragraph on several lines: up and down go by the engine's lines.
  const torture = await open("/tests/fixtures/torture.scaena");
  await showState(torture, "pretty", 12);
  check((await typeIn(torture, "pretty-para")) === "pretty-para", "a double click types in a paragraph");
  const lines = (await typed(torture))?.lines ?? 0;
  check(lines > 2, `the paragraph is set on ${lines} lines`);
  await torture.keyboard.press("Control+End");
  check((await typed(torture))?.line === lines - 1, "at its end, the caret is on the last line");
  await torture.keyboard.press("ArrowUp");
  const up = await typed(torture);
  check(up?.line === lines - 2 && up.from === up.to && up.from < up.value.length, `up goes to the line above: ${JSON.stringify({ ...up, value: undefined })}`);
  await torture.keyboard.press("Home");
  const home = await typed(torture);
  await torture.keyboard.press("End");
  const end = await typed(torture);
  check(home?.line === lines - 2 && end?.line === lines - 2 && home.from < up.from && end.from > up.from, "Home and End go to that line's ends");
  await torture.keyboard.press("ArrowDown");
  check((await typed(torture))?.line === lines - 1, "and down comes back");
  await torture.keyboard.press("Escape");

  // Runs keep their looks as they are typed in.
  await showState(torture, "mixed", 8);
  const plain = await source(torture);
  check((await typeIn(torture, "mixed-line")) === "mixed-line", "a double click types in a text set by runs");
  await torture.keyboard.press("Control+Home");
  await torture.keyboard.type("Very ");
  await reads(torture, '"Very Thin "');
  const runs = await source(torture);
  check(runs.includes('"Very Thin "') && runs.includes('"Regular "') && runs.includes('"Black "'), "the first run takes what is typed at its start, and the others stay as they were");
  await torture.keyboard.press("Control+z");
  check(await back(torture, plain), "one undo takes it back");
} catch (e) {
  failures.push(String(e));
} finally {
  await browser.close();
  server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "text is typed where it stands, and undone");
process.exit(failures.length ? 1 : 0);

// PLAN 2.133 check: code pasted on the editor's canvas, in headless Chromium (serve.mjs), the CPU
// painting, on the revenue example's closing slide.
//
//   node web/code.mjs     (after `just web`; from the repository's root)
//
// - Rust copied from a page, its `<pre>` classed `language-rust`, pastes as a code text in Rust,
//   tidied: its tabs and the indentation its lines share gone, its line ends one. One step to undo.
// - Its chip says Rust; a press on it offers the languages, and Python chosen is one step to undo.
// - Copy Code puts its words on the clipboard as text.
// - Keys typed in it in one burst (PLAN 2.139): Enter a line a stop in after `{`, Shift+Tab a stop
//   out, letters between, each in turn and one step to undo; ⌘B withheld. Not Code, Tidy Code,
//   and Make Code, each from the palette.
// - A walkthrough (PLAN 2.140): Walk Through Lines… adds a step for each range in one step to undo;
//   Clear Focus with none set changes nothing; Focus on These Lines sets the caret's line.
// - Prose pastes as words, and Paste as Text pastes code as words.
// - VS Code's mode names the language of what it copies.
// - A Makefile's keys keep its recipes' tabs.
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
  const context = await browser.newContext({ viewport: { width: 1920, height: 1200 }, deviceScaleFactor: 1, permissions: ["clipboard-read", "clipboard-write"] });
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
  /** ⌘Z, then once the source holds `has` and not `lacks`: one step back, however fast the
   *  steps before it were written. */
  const undoTo = async (has, lacks) => {
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+z");
    const holds = ([has, lacks]) => {
      const s = window.scaena.source();
      return s.includes(has) && !s.includes(lacks) && !window.scaena.canvas.busy();
    };
    await page.waitForFunction(holds, [has, lacks], { timeout: 30000, polling: 50 }).catch(() => {});
    await page.waitForTimeout(300);
    const s = await source();
    return s.includes(has) && !s.includes(lacks);
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
  /** A paste on the canvas, focused, as ⌘V fires it: `data` by media type. */
  const paste = (data) =>
    page.evaluate((data) => {
      const held = new DataTransfer();
      for (const [type, value] of Object.entries(data)) held.setData(type, value);
      const e = new ClipboardEvent("paste", { clipboardData: held, bubbles: true, cancelable: true });
      document.querySelector("#overlay").focus();
      document.querySelector("#overlay").dispatchEvent(e);
      return e.defaultPrevented;
    }, data);

  /** Run the command `label` from the palette, as Ctrl+K and its name typed then Enter run it. */
  const run = async (label) => {
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+k");
    await page.waitForFunction(() => document.querySelector("#palette").open, null, { timeout: 10000 });
    await page.keyboard.type(label);
    await page.keyboard.press("Enter");
  };

  // The closing slide, with room below its title.
  const index = await page.evaluate(() => window.scaena.opened.states.indexOf("close"));
  await page.evaluate((offset) => window.scaena.cursor(offset), (await source()).indexOf("state close") + "state ".length);
  await page.waitForFunction((i) => window.scaena.shown() === i && window.scaena.canvas.boxes().length > 0, index, { timeout: 30000 });
  const original = await source();
  /** The code text the canvas has, as its box says it. */
  const coded = () => page.evaluate(() => window.scaena.canvas.boxes().find((b) => b.code));

  // Rust copied from a page: tabs, CRLF, and an indentation its lines share.
  await press([960, 760]);
  const rust = "\tfn main() {\r\n\t    let total = 42;\r\n\t    println!(\"{total}\");\r\n\t}\r\n";
  const html = '<pre class="language-rust"><code>fn main() {</code></pre>';
  check(await paste({ "text/plain": rust, "text/html": html }), "the canvas takes the code");
  check(await changed(original), "it comes in");
  const pasted = await source();
  check(pasted.includes("rust") && pasted.includes("fn main() {"), "as code in Rust");
  check(!pasted.includes("\t") && !pasted.includes("\r"), "tidied: no tab, no carriage return");
  const box = await coded();
  check(box?.code?.language === "rust" && box?.code?.said === "Rust", `its box says Rust: ${JSON.stringify(box?.code)}`);

  // Its chip, drawn while it is selected, offers the languages.
  check((await selected()) === box?.node, "it is selected");
  const chip = page.locator("#overlay .code-chip");
  await chip.waitFor({ timeout: 30000 }).catch(() => {});
  check((await chip.count()) === 1, "its chip shows");
  check((await chip.textContent())?.includes("Rust"), `the chip says Rust: ${await chip.textContent()}`);
  await chip.dispatchEvent("pointerdown", { button: 0, clientX: 10, clientY: 10, pointerId: 1, isPrimary: true });
  const menu = page.locator(".context-menu");
  await menu.waitFor({ timeout: 30000 }).catch(() => {});
  check((await menu.locator("[role=menuitem]", { hasText: "Python" }).count()) > 0, "the chip offers Python");
  await menu.locator("[role=menuitem]", { hasText: "Python" }).first().click();
  check(await changed(pasted), "a language chosen changes the deck");
  check((await coded())?.code?.language === "python", "it is Python now");
  check(await undo(pasted), "⌘Z takes the language back");

  // Copy Code: its words on the clipboard as text.
  await run("Copy Code");
  await says("The code copied as text");
  const copied = await page.evaluate(() => navigator.clipboard.readText());
  check(copied === "fn main() {\n    let total = 42;\n    println!(\"{total}\");\n}", `Copy Code copies its words: ${JSON.stringify(copied)}`);

  // A walkthrough (PLAN 2.140): Walk Through Lines… asks the lines each step shows, from the whole
  // block to the whole block, and adds a step for each after the first, gliding in, in one patch;
  // ⌘Z takes them back. Clear Focus with none set changes nothing. Focus on These Lines sets the
  // lines the caret touches, in the state shown.
  const states = async () => ((await source()).match(/^state /gm) ?? []).length;
  const before = await states();
  await run("Walk Through Lines…");
  const asked = page.locator(".note-words");
  await asked.waitFor({ timeout: 10000 });
  check((await asked.inputValue()) === "1–4; 1–4", `it asks from the whole block to the whole block: ${await asked.inputValue()}`);
  await asked.fill("1–4; 2–3; 4; 1–4");
  await asked.press("Enter");
  check(await changed(pasted), "Walk Through Lines… changes the deck");
  check(await says("3 steps added"), `a step for each range after the first: ${await status()}`);
  check((await states()) === before + 3, `three steps in the deck: ${await states()}`);
  const walked = await source();
  check(/focus/.test(walked) && /\[2, 3\]/.test(walked), "each step names its lines in focus");
  check(await undo(pasted), "⌘Z takes the walkthrough back in one step");
  await run("Clear Focus");
  check(await says("shows every line already"), `Clear Focus with none set changes nothing: ${await status()}`);
  await page.locator("#overlay").focus();
  await page.keyboard.press("Enter");
  await page.waitForFunction(() => window.scaena.canvas.typing?.() !== undefined, null, { timeout: 30000 }).catch(() => {});
  await page.keyboard.press("Control+Home");
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Escape");
  await run("Focus on These Lines");
  check(await says("lines 2 in focus"), `Focus on These Lines sets the line the caret was on: ${await status()}`);
  check(await changed(pasted), "the focus is set");
  check(await undo(pasted), "⌘Z takes the focus away");

  // Keys typed in code (PLAN 2.139), in one burst: Enter after `fn main() {` is a line a stop in,
  // two letters on it, Shift+Tab a stop out, and a third; the engine answers each key in turn, so
  // none is lost, and the burst is one step to undo.
  await page.locator("#overlay").focus();
  await page.keyboard.press("Enter");
  await page.waitForFunction(() => window.scaena.canvas.typing?.() !== undefined, null, { timeout: 30000 }).catch(() => {});
  await page.waitForTimeout(300);
  await page.keyboard.press("Control+Home");
  await page.keyboard.press("End");
  await page.keyboard.press("Enter");
  await page.keyboard.type("ab");
  await page.keyboard.press("Shift+Tab");
  await page.keyboard.type("c");
  // ⌘B in code is withheld: no characters of it take a look.
  await page.keyboard.press("Control+b");
  check(await says("Code is set as written"), `⌘B in code says code takes no look: ${await status()}`);
  await page.keyboard.press("Escape");
  check(await changed(pasted), "the keys change the code");
  await run("Copy Code");
  await says("The code copied as text");
  const keyed = await page.evaluate(() => navigator.clipboard.readText());
  check(
    keyed === "fn main() {\nabc\n    let total = 42;\n    println!(\"{total}\");\n}",
    `Enter, letters, and Shift+Tab as a code editor's: ${JSON.stringify(keyed)}`,
  );
  check(await undo(pasted), "⌘Z takes the burst back in one step");

  // Not Code makes it words; Tidy Code finds it tidy; Make Code makes prose code (PLAN 2.139).
  await run("Not Code");
  check(await changed(pasted), "Not Code changes the deck");
  check(!(await coded()), "Not Code makes it words");
  check(await undo(pasted), "⌘Z gives the code back");
  await run("Tidy Code");
  check(await says("is tidy already"), `Tidy Code finds it tidy: ${await status()}`);
  check(await undo(original), "⌘Z takes the paste away");
  await press([960, 760]);
  check(await paste({ "text/plain": "Margins held through the quarter." }), "the canvas takes prose to make code of");
  check(await changed(original), "the prose comes in");
  const prose = await source();
  await run("Make Code");
  check(await changed(prose), "Make Code changes the deck");
  check((await coded())?.code !== undefined, `Make Code makes it code: ${JSON.stringify((await coded())?.code)}`);
  check(await undo(prose), "⌘Z gives the words back");
  check(await undo(original), "⌘Z takes the prose away");

  // Prose pastes as words.
  await press([960, 760]);
  check(await paste({ "text/plain": "Margins held through the quarter: costs fell, and prices held." }), "the canvas takes prose");
  check(await changed(original), "the prose comes in");
  check(!(await coded()), "as words, not code");
  check(await undo(original), "⌘Z takes it away");

  // Paste as Text: code as words.
  await page.evaluate((t) => navigator.clipboard.writeText(t), rust);
  await press([960, 760]);
  await run("Paste as Text");
  check(await changed(original), "Paste as Text pastes");
  check(!(await coded()), "code pasted as text is words");
  check(await undo(original), "⌘Z takes it away");

  // VS Code's mode names the language of what it copies.
  await press([960, 760]);
  const sql = "select name, total from orders where total > 100 order by total desc";
  check(await paste({ "text/plain": sql, "vscode-editor-data": JSON.stringify({ version: 1, isFromEmptySelection: false, mode: "sql" }) }), "the canvas takes VS Code's");
  check(await changed(original), "it comes in");
  check((await coded())?.code?.language === "sql", `as SQL, by VS Code's mode: ${JSON.stringify((await coded())?.code)}`);
  check(await undo(original), "⌘Z takes it away");

  // A Makefile's recipe is a tab, and its keys keep it so: Return, Shift+Tab to the line's start,
  // then Tab, which is a tab in a Makefile where elsewhere it is spaces (PLAN 2.139).
  await press([960, 760]);
  const make = "all: build\n\techo built";
  check(await paste({ "text/plain": make, "vscode-editor-data": JSON.stringify({ version: 1, isFromEmptySelection: false, mode: "makefile" }) }), "the canvas takes a Makefile");
  check(await changed(original), "the Makefile comes in");
  const made = await source();
  await page.locator("#overlay").focus();
  await page.keyboard.press("Enter");
  await page.waitForFunction(() => window.scaena.canvas.typing?.() !== undefined, null, { timeout: 30000 }).catch(() => {});
  await page.waitForTimeout(300);
  await page.keyboard.press("Control+End");
  await page.keyboard.press("Enter");
  await page.keyboard.press("Shift+Tab");
  await page.keyboard.press("Tab");
  await page.keyboard.type("echo done");
  await page.keyboard.press("Escape");
  check(await changed(made), "the keys change the Makefile");
  await run("Copy Code");
  await says("The code copied as text");
  const recipe = await page.evaluate(() => navigator.clipboard.readText());
  check(recipe === "all: build\n\techo built\n\techo done", `its recipes keep their tabs: ${JSON.stringify(recipe)}`);
  check(await undo(made), "⌘Z takes the keys back");
  check(await undo(original), "⌘Z takes the Makefile away");
} finally {
  await browser.close();
  await server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "code pastes as code in its language, tidied, which its chip changes");
process.exit(failures.length ? 1 : 0);

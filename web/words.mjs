// PLAN 2.108 check: words in a shape, on the editor's canvas, in headless Chromium (serve.mjs), the
// CPU painting, as in web/canvas.mjs.
//
//   node web/words.mjs     (after `just web`; from the repository's root)
//
// On the revenue example, in `revenue`:
// - A star from the shape library takes words: a double click on it gives it a text, `star-words`,
//   placed by the star alone (`at:parent(star)`) in the color that reads on its fill, and types in
//   it, one step to undo.
// - What is typed stands inside the star; Escape, and a click on the words, select the star.
// - A drag that starts on the words moves the star, and the words go with it, still placed by it.
// - A double click on the star again types in the words it holds: no second text is made.
// - ⌘D copies the star with its words, kept to this slide though the next carries on from it;
//   undo takes the copy back.
// - Undo takes back the drag, the typing, the words, and the star, each one step.
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
  const page = await context.newPage();
  page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
  page.on("console", (m) => m.type() === "error" && failures.push(`console: ${m.text()}`));
  await page.goto(url("/docs/examples/revenue.deck.json"));
  await page.waitForFunction(() => window.scaena?.last() || document.querySelector("#status")?.textContent.startsWith("Error"), null, {
    timeout: 120000,
  });

  const source = () => page.evaluate(() => window.scaena.source());
  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  const typing = () => page.evaluate(() => window.scaena.canvas.typing());
  /** Once the canvas selects `id`: whether it does within the wait. */
  const selects = (id) =>
    page
      .waitForFunction((n) => window.scaena.canvas.selected() === n, id, { timeout: 10000 })
      .then(() => true, () => false);
  const box = (node) => page.evaluate((n) => window.scaena.canvas.boxes().find((b) => b.node === n)?.rect, node);
  /** Once the source holds `text` (or, with `absent`, does not), and the canvas has caught up. */
  const reads = async (text, absent = false) => {
    await page
      .waitForFunction(([t, a]) => window.scaena.source().includes(t) !== a && !window.scaena.canvas.busy(), [text, absent], {
        timeout: 30000,
        polling: 50,
      })
      .catch(() => {});
    await page.waitForTimeout(300);
    return (await source()).includes(text) !== absent;
  };
  const back = async (to) => {
    await page.waitForFunction((s) => window.scaena.source() === s && !window.scaena.canvas.busy(), to, { timeout: 30000, polling: 50 }).catch(() => {});
    await page.waitForTimeout(300);
    return (await source()) === to;
  };
  /** Show the state declared as `state` in the source, slot `index`. */
  const showState = async (state, index) => {
    const at = (await source()).indexOf(`state ${state}`) + "state ".length;
    await page.evaluate((offset) => window.scaena.cursor(offset), at);
    await page.waitForFunction((i) => window.scaena.shown() === i && window.scaena.at().index === i, index, { timeout: 30000 });
    // The canvas's boxes are the shown state's, not the one shown before it.
    await page.waitForFunction((s) => window.scaena.canvas.boxed() === s && window.scaena.canvas.boxes().length > 0, state, { timeout: 30000 });
  };
  /** A canvas point as the page's. */
  const client = ([x, y]) =>
    page.evaluate(
      ([x, y]) => {
        const r = document.querySelector("#overlay").getBoundingClientRect();
        const [w, h] = window.scaena.canvas.size();
        return [r.left + (x / w) * r.width, r.top + (y / h) * r.height];
      },
      [x, y],
    );
  const middle = ([x, y, w, h]) => [x + w / 2, y + h / 2];
  const within = ([x, y, w, h], [X, Y, W, H]) => x >= X - 0.5 && y >= Y - 0.5 && x + w <= X + W + 0.5 && y + h <= Y + H + 0.5;
  /** The declaration of `node` in the source, with the lines its properties run on to, as one:
   * in a state that enters it, or a `node` of its own where none does, as a shape's words. */
  const declared = (scn, node) => {
    const lines = scn.split("\n");
    const line = lines.findIndex((l) => l.trimStart().startsWith(`${node} `) || l.startsWith(`node ${node} `));
    if (line < 0) return "";
    const indent = (l) => l.length - l.trimStart().length;
    const rest = lines.slice(line + 1);
    const end = rest.findIndex((l) => indent(l) <= indent(lines[line]));
    return [lines[line], ...rest.slice(0, end < 0 ? rest.length : end)].map((l) => l.trim()).join(" ");
  };
  /** Double click the canvas at `at`, canvas units, and wait until a text is typed in. */
  const doubleClick = async (at) => {
    const [x, y] = await client(at);
    await page.mouse.dblclick(x, y);
    await page.waitForFunction(() => window.scaena.canvas.typing() !== undefined, null, { timeout: 30000 }).catch(() => {});
    await page.waitForFunction(() => document.querySelector("#status").textContent.startsWith("Typing in"), null, { timeout: 30000 }).catch(() => {});
  };
  /** Leave typing, the canvas focused. */
  const leave = async () => {
    await page.keyboard.press("Escape");
    await page.waitForFunction(() => window.scaena.canvas.typing() === undefined && !window.scaena.canvas.busy(), null, { timeout: 30000 });
    await page.locator("#overlay").focus();
  };

  await showState("revenue", 1);
  const original = await source();

  // A star from the shape library, in the room below the chart.
  const [px, py] = await client([700, 760]);
  await page.mouse.click(px, py);
  await page.selectOption("#insert", { label: "Star" });
  check(await reads("star shape kind:polygon"), "a star is inserted");
  check(await selects("star"), "and selected");
  const starred = await source();
  const star = await box("star");
  check(Boolean(star), `it stands on the canvas: ${star}`);

  // A double click on it gives it words and types in them.
  await doubleClick(middle(star));
  check((await typing()) === "star-words", `a double click on the star types in its words: ${await typing()}`);
  const words = declared(await source(), "star-words");
  check(/^(node )?star-words text/.test(words) && words.includes("at:parent(star)"), `the words are a text placed by the star: ${words}`);
  check(!/rect\(|col\(|in\(/.test(words), `and by nothing else: ${words}`);
  check(words.includes("onAccent"), `in the color that reads on the star's accent: ${words}`);
  const worded = await source();
  await page.keyboard.type("Ship it");
  check(await reads('"Ship it"'), "what is typed is the star's words");
  await leave();
  check(await selects("star"), `Escape leaves the words with the star selected: ${await page.evaluate(() => window.scaena.canvas.selected())}`);
  const typed = await source();
  const inside = await box("star-words");
  check(Boolean(inside) && within(inside, star), `the words stand inside the star: ${inside} in ${star}`);

  // A click on the words selects the star.
  await page.mouse.click(...(await client(middle(inside))));
  check(await selects("star"), `a click on the words selects the star: ${await page.evaluate(() => window.scaena.canvas.selected())}`);

  // A drag that starts on the words moves the star, and the words with it.
  const [fx, fy] = await client(middle(inside));
  const [tx, ty] = await client([middle(inside)[0] - 240, middle(inside)[1] - 120]);
  await page.mouse.move(fx, fy);
  await page.mouse.down();
  for (let i = 1; i <= 8; i++) await page.mouse.move(fx + ((tx - fx) * i) / 8, fy + ((ty - fy) * i) / 8);
  await page.mouse.up();
  await page.waitForFunction((s) => window.scaena.source() !== s && !window.scaena.canvas.busy(), typed, { timeout: 30000, polling: 50 }).catch(() => {});
  await page.waitForTimeout(300);
  const moved = await source();
  const [star2, inside2] = [await box("star"), await box("star-words")];
  check(moved !== typed && declared(moved, "star") !== declared(typed, "star"), "the drag moves the star");
  check(declared(moved, "star-words") === declared(typed, "star-words"), `the words are still placed by it alone: ${declared(moved, "star-words")}`);
  check(Boolean(star2 && inside2) && star2[0] < star[0] && within(inside2, star2), `and stand inside it where it went: ${inside2} in ${star2}`);

  // A double click on the star again types in the words it holds.
  await doubleClick(middle(star2));
  check((await typing()) === "star-words", `a double click on the star types in the words it holds: ${await typing()}`);
  await leave();
  check(!(await source()).includes("star-words-2"), "no second text is made");

  // ⌘D copies the star with its words, though the next slide carries on from this one.
  const held = await source();
  await page.locator("#overlay").focus();
  if ((await page.evaluate(() => window.scaena.canvas.selected())) !== "star") await page.mouse.click(...(await client(middle(star2))));
  check(await selects("star"), `the star is selected: ${await page.evaluate(() => window.scaena.canvas.selected())}`);
  await page.keyboard.press("Control+d");
  check(await reads("star-words-2"), `⌘D copies the star with its words: ${await status()}`);
  check(declared(await source(), "star-words-2").includes("star-2"), `placed by the copy: ${declared(await source(), "star-words-2")}`);
  await page.keyboard.press("Control+z");
  check(await back(held), "undo takes the copy back");

  // Each step undone.
  await page.keyboard.press("Control+z");
  check(await back(typed), "undo takes the drag back");
  await page.keyboard.press("Control+z");
  check(await back(worded), "then the typing");
  await page.keyboard.press("Control+z");
  check(await back(starred), "then the words");
  await page.keyboard.press("Control+z");
  check(await back(original), "then the star");
  check(await page.evaluate(() => window.scaena.last().valid), "the source still compiles and validates");
} finally {
  await browser.close();
  await server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "a shape takes words, which go where it goes");
process.exit(failures.length ? 1 : 0);

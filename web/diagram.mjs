// PLAN 2.110 check: a text's lines drawn as a diagram, on the editor's canvas, in headless Chromium
// (serve.mjs), the CPU painting, as in web/canvas.mjs.
//
//   node web/diagram.mjs     (after `just web`; from the repository's root)
//
// On the revenue example, in `intro`:
// - The subtitle typed in as three lines: Plan, Build, and Ship.
// - The palette's Make a process diagram draws them in the subtitle's place: `subtitle-process`, a
//   stack of three boxes, each holding its line, joined by two arrows, selected; the subtitle gone
//   from the slide. One undo takes it all back.
// - A right click on the subtitle offers Make a cycle diagram: the three boxes on a ring, joined by
//   three arrows. One undo takes it back.
// - Make a hierarchy diagram refuses lines at one level, and says to indent them.
// - The process is listed where the subtitle was, before the chart, so it paints and reads in the
//   subtitle's turn; and a locked subtitle is drawn as no diagram, the status saying why (PLAN 2.95).
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
  const selected = () => page.evaluate(() => window.scaena.canvas.selected());
  /** Once the canvas selects `id`: whether it does within the wait. */
  const selects = (id) =>
    page
      .waitForFunction((n) => window.scaena.canvas.selected() === n, id, { timeout: 10000 })
      .then(() => true, () => false);
  const boxes = () => page.evaluate(() => window.scaena.canvas.boxes());
  /** Once the source holds `text`, and the canvas has caught up. */
  const reads = async (text) => {
    await page
      .waitForFunction((t) => window.scaena.source().includes(t) && !window.scaena.canvas.busy(), text, { timeout: 30000, polling: 50 })
      .catch(() => {});
    await page.waitForTimeout(300);
    return (await source()).includes(text);
  };
  const back = async (to) => {
    await page.waitForFunction((s) => window.scaena.source() === s && !window.scaena.canvas.busy(), to, { timeout: 30000, polling: 50 }).catch(() => {});
    await page.waitForTimeout(300);
    return (await source()) === to;
  };
  /** Once the status line says `words`: whether it does within the wait. */
  const says = (words) =>
    page
      .waitForFunction((w) => document.querySelector("#status").textContent.includes(w), words, { timeout: 30000, polling: 50 })
      .then(() => true, () => false);
  /** Once the canvas draws `node` again: whether it does within the wait. */
  const drawsAgain = (node) =>
    page
      .waitForFunction((n) => window.scaena.canvas.boxes()?.some((b) => b.node === n), node, { timeout: 30000, polling: 50 })
      .then(() => true, () => false);
  /** The client point at the middle of `node`'s box, once the canvas has one for it. */
  const middle = async (node) => {
    await page.waitForFunction((n) => window.scaena.canvas.boxes().some((x) => x.node === n), node, { timeout: 10000 }).catch(() => {});
    return page.evaluate((n) => {
      const b = window.scaena.canvas.boxes().find((x) => x.node === n);
      const r = document.querySelector("#overlay").getBoundingClientRect();
      const [w, h] = window.scaena.canvas.size();
      const [x, y, bw, bh] = b.rect;
      return [r.left + ((x + bw / 2) / w) * r.width, r.top + ((y + bh / 2) / h) * r.height];
    }, node);
  };
  /** The declaration of `node` in the source, with the lines its properties run on to, as one. */
  const declared = (scn, node) => {
    const lines = scn.split("\n");
    const line = lines.findIndex((l) => l.trimStart().startsWith(`${node} `) || l.startsWith(`node ${node} `));
    if (line < 0) return "";
    const indent = (l) => l.length - l.trimStart().length;
    const rest = lines.slice(line + 1);
    const end = rest.findIndex((l) => indent(l) <= indent(lines[line]));
    return [lines[line], ...rest.slice(0, end < 0 ? rest.length : end)].map((l) => l.trim()).join(" ");
  };
  /** The palette's command `label`, found by its words, run on what is selected. */
  const command = async (label) => {
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+k");
    await page.waitForFunction(() => document.querySelector("#palette").open, null, { timeout: 10000 });
    await page.keyboard.type(label.toLowerCase());
    const listed = await page.evaluate(() => window.scaena.commands.shown());
    check(listed[0] === label, `the palette offers ${label} first: ${listed.slice(0, 3)}`);
    await page.keyboard.press("Enter");
  };

  // In `intro`.
  await page.evaluate(() => window.scaena.cursor(window.scaena.source().indexOf("state intro") + "state ".length));
  await page.waitForFunction(() => window.scaena.shown() === 0 && window.scaena.canvas.boxed() === "intro", null, { timeout: 30000 });

  // The subtitle typed in as three lines.
  await page.mouse.dblclick(...(await middle("subtitle")));
  await page.waitForFunction(() => window.scaena.canvas.typing() === "subtitle", null, { timeout: 30000 }).catch(() => {});
  check((await page.evaluate(() => window.scaena.canvas.typing())) === "subtitle", "a double click types in the subtitle");
  await page.keyboard.press("Control+a");
  await page.keyboard.type("Plan");
  await page.keyboard.press("Enter");
  await page.keyboard.type("Build");
  await page.keyboard.press("Enter");
  await page.keyboard.type("Ship");
  check(await reads("Ship"), "the subtitle takes three lines");
  await page.keyboard.press("Escape");
  check(await selects("subtitle"), `and stays selected: ${await selected()}`);
  const lines = await source();

  // A process, from the palette.
  await command("Make a process diagram");
  check(await reads("subtitle-process"), `the subtitle is drawn as a process: ${await status()}`);
  const drawn = await source();
  check(/stack/.test(declared(drawn, "subtitle-process")), `in a stack: ${declared(drawn, "subtitle-process")}`);
  const words = [1, 2, 3].map((k) => declared(drawn, `subtitle-process-${k}-words`));
  check(
    ["Plan", "Build", "Ship"].every((w, k) => words[k].includes(`"${w}"`)),
    `a box for each line, holding it: ${words.join(" | ")}`,
  );
  check(await selects("subtitle-process"), `the diagram is selected: ${await selected()}`);
  const shown = await boxes();
  const held = shown.filter((b) => b.parent === "subtitle-process").map((b) => b.node);
  check(held.length === 3, `three boxes in it: ${held}`);
  const arrows = shown.filter((b) => b.joins?.every((j) => held.includes(j)));
  check(arrows.length === 2, `joined by two arrows: ${arrows.map((a) => a.node)}`);
  check(!shown.some((b) => b.node === "subtitle"), "the subtitle gone from the slide");
  const line = (node) => drawn.split("\n").findIndex((l) => l.trimStart().startsWith(`${node} `) || l.startsWith(`node ${node} `));
  check(line("subtitle-process") >= 0 && line("subtitle-process") < line("rev"), `listed in the subtitle's turn, before the chart: ${line("subtitle-process")} ${line("rev")}`);
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+z");
  check(await back(lines), "one undo takes the diagram back");
  check(await drawsAgain("subtitle"), "and the subtitle stands on the slide again");

  // A cycle, from a right click on the subtitle.
  await page.mouse.click(...(await middle("subtitle")), { button: "right" });
  await page.waitForFunction(() => document.querySelector(".context-menu"), null, { timeout: 10000 }).catch(() => {});
  const offered = await page.evaluate(() => [...document.querySelectorAll(".context-menu [role=menuitem] span")].map((s) => s.textContent));
  check(offered.includes("Make a cycle diagram"), `a right click on it offers a cycle: ${offered.join(", ")}`);
  await page.locator(".context-menu [role=menuitem]", { hasText: "Make a cycle diagram" }).first().click();
  check(await reads("subtitle-cycle"), `the subtitle is drawn as a cycle: ${await status()}`);
  const ring = await boxes();
  const around = ring.filter((b) => b.parent === "subtitle-cycle").map((b) => b.node);
  const loops = ring.filter((b) => b.joins?.every((j) => around.includes(j)));
  check(around.length === 3 && loops.length === 3, `three boxes on a ring, three arrows round it: ${around} ${loops.map((l) => l.node)}`);
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+z");
  check(await back(lines), "one undo takes it back");
  check(await drawsAgain("subtitle"), "and the subtitle stands there again");

  // A hierarchy, refused for lines at one level.
  await page.mouse.click(...(await middle("subtitle")));
  check(await selects("subtitle"), `a click selects the subtitle: ${await selected()}`);
  await command("Make a hierarchy diagram");
  check(await says("lines under others"), `a hierarchy says to indent the lines under others: ${await status()}`);
  check((await source()) === lines, "and draws nothing");
  check(await page.evaluate(() => window.scaena.last().valid), "the source still compiles and validates");

  // Locked, the subtitle is drawn as no diagram: the canvas deletes nothing of what is locked.
  await page.locator("#overlay").focus();
  await page.keyboard.press("Control+Shift+L");
  check(await reads("locked"), `⇧⌘L locks the subtitle: ${await status()}`);
  const locked = await source();
  await command("Make a process diagram");
  check(await says("locked"), `a locked subtitle says why it is drawn as no diagram: ${await status()}`);
  await page.waitForTimeout(500);
  check((await source()) === locked, "and nothing is drawn");
} finally {
  await browser.close();
  await server.close();
}
console.log(failures.length ? `${failures.length} failure(s): ${failures.join("; ")}` : "a text's lines are drawn as a diagram in its place");
process.exit(failures.length ? 1 : 0);

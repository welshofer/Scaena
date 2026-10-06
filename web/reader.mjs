// PLAN 2.56 check: what a reader hears, from the editor's inspector, in headless Chromium
// (serve.mjs), the CPU painting, on the trails example.
//
//   node web/reader.mjs     (after `just web`; from the repository's root)
//
// - Every node offers its description (`alt`) and its part in the story (`semantic`): a text's
//   description says what to say for it instead of its words, and is empty, its words read; its
//   part is one of the schema's words, and shows where it lives.
// - Under the choices, what the node shown reads as, as the player's live region says it: a
//   heading or a text by its words, a figure by its description; decoration is not read.
// - With nothing selected, the state shown reads part by part, in order, and a part selects its
//   node.
// - Each is a `choose`, by the user, written where it lives, one step to undo: a description
//   takes the place of the words, and decoration takes the node out of the reading.
// - An image that is not described is W410: its mark offers to describe it, which opens the
//   inspector on it with its description in focus; described, the mark goes, and the image reads
//   by its description.
// - axe-core finds nothing against WCAG 2.1 AA in the inspector.
// Exits 1 on any failure.
import { createRequire } from "node:module";
import { launch, serve } from "./serve.mjs";

const server = await serve();
const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};
const axe = createRequire(import.meta.url).resolve("axe-core/axe.min.js");

const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1920, height: 1200 }, deviceScaleFactor: 1 });
  const page = await context.newPage();
  page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
  page.on("console", (m) => m.type() === "error" && failures.push(`console: ${m.text()}`));
  await page.goto(`${server.origin}/web/dist/editor.html?painter=cpu&bundle=/docs/examples/trails.deck.json`);
  await page.waitForFunction(() => window.scaena?.last(), null, { timeout: 120000 });

  const source = () => page.evaluate(() => window.scaena.source());
  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  const field = (prop) => page.evaluate((p) => window.scaena.look.offered()?.fields.find((f) => f.prop === p), prop);
  /** What the inspector says the node shown, or the state, reads as. */
  const heard = () => page.evaluate(() => document.querySelector("#look .reads")?.textContent ?? "");
  /** Once `test` holds (polled in the page), and the page has settled. */
  const until = async (test, arg, timeout = 30000) => {
    const ok = await page.waitForFunction(test, arg, { timeout, polling: 50 }).then(() => true, () => false);
    await page.waitForTimeout(200);
    return ok;
  };
  /** State `id` shown, the cursor on its declaration, what stands where in it known. */
  const into = async (id) => {
    await page.evaluate((s) => window.scaena.cursor(window.scaena.source().indexOf(`state ${s}`) + "state ".length), id);
    await until((s) => window.scaena.canvas.boxed() === s && window.scaena.canvas.boxes().length > 0, id);
  };
  /** Select `node` in the state shown, and wait for the inspector to show it and how it reads. */
  const select = async (node) => {
    await page.evaluate((n) => window.scaena.canvas.select(n), node);
    await until((n) => window.scaena.look.offered()?.node === n && document.querySelector("#look .reads")?.textContent, node);
  };
  /** Undo, as the keys do on the canvas. */
  const undo = async () => {
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+z");
  };
  /** Type `text` into the inspector's field for `prop`, and leave it, as Tab does: one choice. */
  const describe = async (prop, text) => {
    await page.fill(`#look-${prop}`, text);
    await page.locator(`#look-${prop}`).press("Tab");
    await page.evaluate(() => window.scaena.look.settled());
  };

  // The cover: its title, a display text that reads as the first heading.
  await into("cover");
  const original = await source();
  await select("cover-title");
  const alt = await field("alt");
  const part = await field("semantic");
  check(alt?.takes.kind === "text" && alt.value === undefined, `a text offers a description, none set: ${JSON.stringify(alt)}`);
  check(
    part?.takes.kind === "word" && part.takes.words.includes("decoration") && part.value === "navigation" && part.lives === "node",
    `and its part, one of the schema's words, as it lives: ${JSON.stringify(part)}`,
  );
  check((await page.getAttribute("#look-alt", "placeholder")) === "its words", "an empty description says the text's words are read");
  check((await heard()).includes("heading 1") && (await heard()).includes("High Country Trails"), `it reads as the first heading, by its words: ${await heard()}`);

  // Described: the description is read instead of the words, written on the node, one step to undo.
  await describe("alt", "High Country Trails, the annual meeting");
  check(await until(() => window.scaena.source().includes('alt:"High Country Trails, the annual meeting"')), "a description is one choose, in the source");
  check(await until(() => document.querySelector("#look .reads")?.textContent.includes("the annual meeting")), `and is what the title reads as: ${await heard()}`);
  check((await status()).includes("cover-title's alt"), `the status says so: ${await status()}`);
  await undo();
  check(await until((s) => window.scaena.source() === s, original), "one undo takes it back");

  // Decoration is not read.
  await select("cover-bg");
  check((await heard()).includes("not read: decoration"), `a backdrop marked decoration is not read: ${await heard()}`);

  // With nothing selected, the state reads part by part, in order; a part selects its node.
  await page.evaluate(() => window.scaena.canvas.select(undefined));
  await until(() => document.querySelector("#look .reads ol li") && !document.querySelector("#look .reads").textContent.includes("annual meeting"));
  const parts = await page.evaluate(() => [...document.querySelectorAll("#look .reads li")].map((li) => li.textContent));
  check(
    parts.length >= 3 && parts.findIndex((p) => p.includes("High Country Trails")) < parts.findIndex((p) => p.includes("What a summer")),
    `the state reads in order: ${JSON.stringify(parts)}`,
  );
  check(!parts.some((p) => p.includes("cover-bg")), "and its decoration is not among them");
  await page.locator('#look .reads [data-read="cover-sub"]').click();
  check(await until(() => window.scaena.look.offered()?.node === "cover-sub"), "a part selects its node");

  // A part chosen as decoration leaves the reading, written where the part lives.
  await page.selectOption("#look-semantic", "decoration");
  await page.evaluate(() => window.scaena.look.settled());
  check(await until(() => /cover-sub text[^\n]*semantic:decoration/.test(window.scaena.source())), "decoration is one choose, on the node");
  check(await until(() => document.querySelector("#look .reads")?.textContent.includes("not read: decoration")), `and the subtitle is no longer read: ${await heard()}`);
  await undo();
  const undone = await until((s) => window.scaena.source() === s, original);
  const now = await source();
  const first = [...now].findIndex((c, i) => c !== original[i]);
  check(undone, `one undo takes it back${undone ? "" : `: differs at ${first}: ${JSON.stringify(now.slice(first - 40, first + 60))} vs ${JSON.stringify(original.slice(first - 40, first + 60))}`}`);

  // axe-core finds nothing in the inspector, its reading shown.
  await select("cover-title");
  await page.addScriptTag({ path: axe });
  const found = await page.evaluate(async () =>
    (await window.axe.run({ include: [["#look"]] }, { runOnly: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"] })).violations.map(
      (v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`,
    ),
  );
  check(!found.length, `axe finds nothing in the inspector${found.length ? `: ${found.join("; ")}` : ""}`);

  // An image not described is W410: its mark offers to describe it.
  const bare = original.replace(/ alt:"Layered ridgelines[^"]*"/, "");
  check(bare !== original, "the storm's photo is described in the example");
  await page.evaluate((s) => window.scaena.type(s), bare);
  await until((s) => window.scaena.source() === s && window.scaena.last()?.whole, bare, 60000);
  await into("storm");
  check(await until(() => window.scaena.canvas.marks().some((m) => m.node === "storm-photo")), `W410 marks the photo: ${JSON.stringify(await page.evaluate(() => window.scaena.canvas.marks()))}`);
  await page.locator('#marks [data-mark="storm-photo"]').click();
  await until(() => document.querySelector("#marked").matches(":popover-open"));
  const offer = page.locator("#marked [data-edit]");
  check((await offer.count()) === 1 && (await offer.textContent()) === "Describe it", "the mark offers to describe it");
  await offer.click();
  check(
    await until(() => window.scaena.look.offered()?.node === "storm-photo" && document.activeElement?.id === "look-alt"),
    `which opens the inspector on the photo, its description in focus (focus on ${await page.evaluate(() => document.activeElement?.id)})`,
  );
  check(
    (await page.evaluate(() => document.querySelector("#tab-inspector").getAttribute("aria-selected"))) === "true",
    "the inspector's tab is shown",
  );
  check((await heard()).includes("figure") && (await heard()).includes("not described"), `it reads as a figure, not described: ${await heard()}`);
  check((await page.getAttribute("#look-alt", "placeholder")) === "not described", "and its empty description says so");
  await page.keyboard.type("Ridgelines at dusk above Bear Creek");
  await page.keyboard.press("Tab");
  await page.evaluate(() => window.scaena.look.settled());
  check(await until(() => window.scaena.source().includes('alt:"Ridgelines at dusk above Bear Creek"')), "described, in the source");
  check(await until(() => !window.scaena.canvas.marks().some((m) => m.node === "storm-photo"), undefined, 60000), "and its mark is gone");
  check(await until(() => document.querySelector("#look .reads")?.textContent.includes("Ridgelines at dusk")), `it reads by its description: ${await heard()}`);
} finally {
  await browser.close();
  server.close();
}

if (failures.length) {
  console.error(`\n${failures.length} failed:\n${failures.map((f) => `  ${f}`).join("\n")}`);
  process.exit(1);
}
console.log("\nall reader checks pass");

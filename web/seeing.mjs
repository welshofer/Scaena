// PLAN 2.52 check: the assistant sees what is selected, in headless Chromium (serve.mjs), the CPU
// painting, on the revenue example, with Anthropic's Messages API played by a scripted server.
//
//   node web/seeing.mjs     (after `just web`; from the repository's root)
//
// - A question begins with what the editor shows, in brackets: the state shown, and nothing
//   selected; then the node selected, with its type.
// - What the assistant's patch changes is selected after it, one step to undo: a title made
//   shorter stays selected, and a note it changes next is selected in its place.
// - Characters selected in a text go with the question, counted as `replace_text` counts them,
//   with the text they make; asking keeps them selected, and a `style_text` the model sends with
//   those offsets gives them that look.
// Exits 1 on any failure.
import { createServer } from "node:http";
import { launch, serve } from "./serve.mjs";

const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};

/** What the model sends back to each question, by its words: a call, or straight an answer. */
const SCRIPT = {
  "What is on this slide?": undefined,
  "Make this shorter.": { ops: [{ op: "replace_text", node: "title", state: "revenue", from: 0, to: 15, text: "Revenue 2x" }] },
  "Say where the numbers come from in the note.": {
    ops: [{ op: "set_text", node: "note", text: "Revenue in $M, from the finance ledger." }],
  },
  "Make these bold.": { ops: [{ op: "style_text", node: "title", state: "revenue", from: 0, to: 7, look: { "style/weight": 700 } }] },
};

/** The words of the last user message, a string or text blocks; none for tool results. */
function words(message) {
  if (typeof message?.content === "string") return message.content;
  const blocks = message?.content ?? [];
  if (blocks.some((b) => b.type === "tool_result")) return undefined;
  return blocks.filter((b) => b.type === "text").map((b) => b.text).join("");
}

/** A scripted Anthropic on a port of its own: each question, as the model got it. */
async function provider() {
  const asked = [];
  let calls = 0;
  const server = createServer(async (req, res) => {
    const cors = {
      "access-control-allow-origin": req.headers.origin ?? "*",
      "access-control-allow-headers": req.headers["access-control-request-headers"] ?? "*",
      "access-control-allow-methods": "GET, POST, OPTIONS",
    };
    if (req.method === "OPTIONS") return res.writeHead(204, cors).end();
    const path = new URL(req.url, "http://x").pathname;
    const reply = (body) => res.writeHead(200, { ...cors, "content-type": "application/json" }).end(JSON.stringify(body));
    if (req.method === "GET" && path === "/v1/models") return reply({ data: [{ id: "scripted", type: "model" }] });
    let text = "";
    for await (const chunk of req) text += chunk;
    const body = JSON.parse(text);
    const usage = { input_tokens: 100, output_tokens: 10 };
    const question = words(body.messages.at(-1));
    if (question === undefined) return reply({ content: [{ type: "text", text: "Done." }], stop_reason: "end_turn", usage });
    asked.push(question);
    const call = Object.entries(SCRIPT).find(([q]) => question.endsWith(q))?.[1];
    if (!call) return reply({ content: [{ type: "text", text: "A chart of revenue by quarter, and its claim." }], stop_reason: "end_turn", usage });
    reply({ content: [{ type: "tool_use", id: `toolu_${calls++}`, name: "deck_patch", input: call }], stop_reason: "tool_use", usage });
  }).listen(0, "127.0.0.1");
  await new Promise((ok) => server.once("listening", ok));
  return { origin: `http://127.0.0.1:${server.address().port}`, asked, close: () => server.close() };
}

const site = await serve();
const scripted = await provider();
const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1 });
  const page = await context.newPage();
  page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
  page.on("console", (m) => m.type() === "error" && !/Failed to load resource/.test(m.text()) && failures.push(`console: ${m.text()}`));
  await page.goto(`${site.origin}/web/dist/editor.html?painter=cpu&bundle=/docs/examples/revenue.deck.json`);
  await page.waitForFunction(() => window.scaena?.last(), null, { timeout: 120000 });

  const source = () => page.evaluate(() => window.scaena.source());
  const selected = () => page.evaluate(() => window.scaena.canvas.selected());
  /** Once the editor has linted the source as it stands, and takes typing again. */
  const settled = () =>
    page.waitForFunction(
      () => {
        const last = window.scaena.last();
        return last && last.whole && !document.querySelector("#code .cm-content[contenteditable=false]");
      },
      null,
      { timeout: 60000, polling: 100 },
    );
  const back = async (to) => {
    await page.waitForFunction((s) => window.scaena.source() === s, to, { timeout: 30000, polling: 50 }).catch(() => {});
    return (await source()) === to;
  };
  const undo = async () => {
    await page.locator("#overlay").focus();
    await page.keyboard.press("Control+z");
  };
  const ask = (text) => page.evaluate((t) => window.scaena.assistant.ask(t), text);

  // The `revenue` state shown, the assistant at the scripted server.
  const shown = await page.evaluate(() => window.scaena.opened.states.indexOf("revenue"));
  await page.evaluate(() => window.scaena.cursor(window.scaena.source().indexOf("state revenue") + "state ".length));
  await page.waitForFunction((i) => window.scaena.shown() === i && window.scaena.canvas.boxed() === "revenue", shown, { timeout: 30000 });
  await page.click("#tab-assistant");
  await page.selectOption("#provider", "anthropic");
  await page.fill("#base", scripted.origin);
  await page.fill("#key", "seeing-test-key");
  await page.waitForFunction(() => document.querySelector("#model").options.length > 0, null, { timeout: 30000 });
  const original = await source();

  // Nothing selected: the question says the state shown, and that.
  let done = await ask("What is on this slide?");
  check(done.kind === "done", `a question with nothing selected is answered: ${JSON.stringify(done)}`);
  check(scripted.asked.at(-1)?.startsWith("[In the editor: state revenue shown; nothing selected.]"), `it says the state shown: ${JSON.stringify(scripted.asked.at(-1))}`);

  // The title selected: the question names it, and the patch that changes it leaves it selected.
  await page.evaluate(() => window.scaena.canvas.select("title"));
  done = await ask("Make this shorter.");
  check(done.kind === "done", `"make this shorter" is answered: ${JSON.stringify(done)}`);
  check(scripted.asked.at(-1)?.startsWith("[In the editor: state revenue shown; selected: title (text).]"), `it names the node selected: ${JSON.stringify(scripted.asked.at(-1))}`);
  await settled();
  const shorter = await source();
  check(/title "Revenue 2x" role:headline/.test(shorter), "the title is shorter in the source, where `revenue` sets it");
  check((await selected()) === "title", `the title stays selected: ${await selected()}`);

  // What the next patch changes is selected in its place.
  done = await ask("Say where the numbers come from in the note.");
  check(done.kind === "done", `the next question is answered: ${JSON.stringify(done)}`);
  await settled();
  check((await source()).includes("from the finance ledger"), "the note says where the numbers come from");
  await page.waitForFunction(() => window.scaena.canvas.selected() === "note", null, { timeout: 30000 }).catch(() => {});
  check((await selected()) === "note", `what the patch changed is selected after it: ${await selected()}`);
  await undo();
  check(await back(shorter), "one undo takes the note's edit back");
  await undo();
  check(await back(original), "and one more the title's");

  // Characters selected in the title: the question carries them, and asking keeps them selected.
  await page.evaluate(() => window.scaena.canvas.select("title"));
  await page.locator("#overlay").focus();
  await page.keyboard.press("Enter");
  await page.waitForFunction(() => window.scaena.canvas.typing() === "title", null, { timeout: 30000 }).catch(() => {});
  await page.keyboard.press("Home");
  for (let i = 0; i < 7; i++) await page.keyboard.press("Shift+ArrowRight");
  const typed = await page.evaluate(() => window.scaena.canvas.typed());
  check(typed?.from === 0 && typed?.to === 7, `"Revenue" is selected in the title: ${JSON.stringify(typed)}`);
  await page.click("#question");
  check((await page.evaluate(() => window.scaena.canvas.typing())) === "title", "focus in the assistant keeps the title typed in");
  await page.fill("#question", "Make these bold.");
  await page.keyboard.press("Enter");
  await page.waitForFunction(() => window.scaena.assistant.transcript().some((t) => t === "said: Done."), null, { timeout: 60000 }).catch(() => {});
  await page.waitForFunction(() => window.scaena.assistant.transcript().filter((t) => t === "said: Done.").length >= 3, null, { timeout: 60000 }).catch(() => {});
  const characters = scripted.asked.at(-1) ?? "";
  check(
    characters.startsWith('[In the editor: state revenue shown; selected: title (text); in title, characters 0 to 7 selected: "Revenue".]'),
    `the question carries the characters selected: ${JSON.stringify(characters)}`,
  );
  await settled();
  const bold = await source();
  check(/\{text: "?Revenue"?, style: \{weight: 700\}\}/.test(bold), "the characters take the look the model gave them");
  await page.keyboard.press("Escape");
  await undo();
  check(await back(original), "one undo takes it back");
} finally {
  await browser.close();
  await site.close();
  scripted.close();
}

if (failures.length) {
  console.log(`\n${failures.length} failed`);
  process.exit(1);
}
console.log("\nseeing: all passed");

// PLAN 2.101 writing check: the editor's writing tools in headless Chromium (serve.mjs), the
// model played by a scripted server that speaks Anthropic's Messages API. On the revenue example:
//
// - With no key, a tool asks nothing, and says where a key goes.
// - Tighten, on the intro's subtitle, asks once with nothing to call: the engine's instructions,
//   the words as the slide shows them. The answer is offered without the quotes the model wrapped
//   it in; Use it makes the text read so, one patch whose words kept stay as they were; ⌘Z puts it
//   back.
// - An answer edited in the dialog is taken as edited; Leave it changes nothing.
// - Draft Notes, with nothing selected, asks from the slide's words and its beat's claim, and
//   writes the slide's notes.
// - No answer from the model offers nothing to take, and says so.
// - Explain, on a finding, says what it means for the slide, and only reads; the source's own
//   issue, while it does not compile, too.
// - Describe (PLAN 2.119), on the torture deck's black-and-white photo, shows the model the picture
//   as the slide shows it, a PNG before the words, gray; Use it writes the picture's description,
//   one patch; ⌘Z puts it back.
//
//   node web/writing.mjs     (after `just web`; from the repository's root)
//
// Exits 1 on any failure.
import { createServer } from "node:http";
import { launch, serve } from "./serve.mjs";

const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};

/** What the scripted model answers, by what its instructions say it does. */
const ANSWERS = [
  ["You tighten", "“This quarter reshaped the business.”"],
  ["You proofread", "This quarter changed the shape of the business."],
  ["You rewrite", "The business took a new shape this quarter."],
  ["You draft speaker notes", "Open on the quarter: the business changed shape, and the rest of the talk says how."],
  ["You explain", "The title's words run past the box its slot gives them. Shorten it, or let it shrink to fit."],
  ["You describe a picture", "“A gray gradient, a dark block, and a lighter disc.”"],
];

/** Words the scripted model, asked to rewrite them, answers nothing for. */
const SILENT = "Q3 Review";

/** Anthropic's Messages API, scripted, on a port of its own: each request it took. */
async function provider() {
  const requests = [];
  const server = createServer(async (req, res) => {
    const cors = {
      "access-control-allow-origin": req.headers.origin ?? "*",
      "access-control-allow-headers": req.headers["access-control-request-headers"] ?? "*",
      "access-control-allow-methods": "GET, POST, OPTIONS",
    };
    if (req.method === "OPTIONS") return res.writeHead(204, cors).end();
    const path = new URL(req.url, "http://x").pathname;
    const reply = (status, body) => res.writeHead(status, { ...cors, "content-type": "application/json" }).end(JSON.stringify(body));
    if (req.method === "GET" && path === "/v1/models") return reply(200, { data: [{ id: "scripted", type: "model" }] });
    if (req.method !== "POST" || path !== "/v1/messages") return reply(404, { error: { message: `no ${req.method} ${path}` } });
    let text = "";
    for await (const chunk of req) text += chunk;
    const body = JSON.parse(text);
    requests.push({ headers: req.headers, body });
    const system = body.system?.[0]?.text ?? "";
    const asked = body.messages?.[0]?.content?.find?.((c) => c.type === "text")?.text ?? "";
    // The model answers nothing for the title's words, as one that refuses or runs out of room does.
    const answer = system.startsWith("You rewrite") && asked === SILENT ? "" : (ANSWERS.find(([starts]) => system.startsWith(starts))?.[1] ?? "I do not know that task.");
    reply(200, { content: [{ type: "text", text: answer }], stop_reason: "end_turn", usage: { input_tokens: 120, output_tokens: 20 } });
  }).listen(0, "127.0.0.1");
  await new Promise((ok) => server.once("listening", ok));
  return { origin: `http://127.0.0.1:${server.address().port}`, requests, close: () => server.close() };
}

const site = await serve();
const scripted = await provider();
const browser = await launch();
try {
  const page = await browser.newPage({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1 });
  page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
  page.on("console", (m) => m.type() === "error" && !/Failed to load resource/.test(m.text()) && failures.push(`console: ${m.text()}`));
  await page.goto(`${site.origin}/web/dist/editor.html?painter=cpu&bundle=/docs/examples/revenue.deck.json`);
  await page.waitForFunction(() => window.scaena?.last()?.valid && window.scaena.canvas.boxes()?.length, null, { timeout: 120000 });

  const source = () => page.evaluate(() => window.scaena.source());
  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  const original = await source();
  const dialog = () =>
    page.evaluate(() => {
      const d = document.querySelector("#writing");
      const shown = (id) => !document.querySelector(id).hidden;
      return {
        open: d.open,
        title: document.querySelector("#writing-title").textContent,
        was: shown("#writing-was") ? document.querySelector("#writing-was").textContent : undefined,
        thinking: shown("#writing-thinking") ? document.querySelector("#writing-thinking").textContent : undefined,
        answer: shown("#writing-answer") ? document.querySelector("#writing-answer").value : undefined,
        said: shown("#writing-said") ? document.querySelector("#writing-said").textContent : undefined,
        by: document.querySelector("#writing-by").textContent,
        use: shown("#writing-use"),
        key: shown("#writing-key"),
        leave: document.querySelector("#writing-leave").textContent,
      };
    });
  /** The inspector on `node` in the state shown, or on the state with none: its writing button `kind` pressed. */
  const tool = async (kind, node) => {
    await page.click("#tab-inspector");
    await page.evaluate((n) => window.scaena.canvas.select(n), node);
    const button = page.locator(`#look [data-write="${kind}"]`);
    await button.waitFor({ timeout: 30000 });
    await button.click();
  };
  const answered = (what) => page.waitForFunction((w) => !document.querySelector(w).hidden, what, { timeout: 30000 }).then(() => true, () => false);

  // 1. No key: nothing is asked, and the dialog says where one goes.
  await tool("tighten", "subtitle");
  await page.waitForFunction(() => document.querySelector("#writing").open, null, { timeout: 10000 });
  const keyless = await dialog();
  check(keyless.key && /key in the Assistant tab/.test(keyless.thinking ?? ""), `with no key, it says where one goes: ${keyless.thinking}`);
  check(scripted.requests.length === 0, "and asks nothing");
  await page.click("#writing-key");
  check(
    await page.evaluate(() => !document.querySelector("#writing").open && document.activeElement?.id === "key"),
    "Add a key… opens the assistant on its key",
  );

  // The assistant's provider, at the scripted server, with a key.
  await page.selectOption("#provider", "anthropic");
  await page.fill("#base", scripted.origin);
  await page.fill("#key", "writing-test-key");
  await page.waitForFunction(() => document.querySelector("#model").options.length > 0, null, { timeout: 30000 });

  // 2. Tighten: asked once, nothing to call; offered; taken as one patch; undone.
  await tool("tighten", "subtitle");
  check(await answered("#writing-answer"), "Tighten offers an answer");
  const tightened = await dialog();
  check(tightened.title === "Tightened" && tightened.was === "This quarter changed the shape of the business.", `titled, with the words as they are: ${tightened.title} · ${tightened.was}`);
  check(tightened.answer === "This quarter reshaped the business.", `the answer, its quotes off: ${tightened.answer}`);
  check(/Written by scripted, with your Anthropic key/.test(tightened.by), `and says who wrote it: ${tightened.by}`);
  const [asked] = scripted.requests;
  check(asked?.headers["x-api-key"] === "writing-test-key", "the key goes to the provider in its own header");
  check(asked && !("tools" in asked.body), "a question asked once sends no tools");
  check(asked?.body.system?.[0]?.text.startsWith("You tighten the words of one text on a slide"), "told the engine's instructions");
  check(asked?.body.messages?.[0]?.content?.[0]?.text === "This quarter changed the shape of the business.", "asked the words as the slide shows them");
  await page.click("#writing-use");
  await page.evaluate(() => window.scaena.writing.settled());
  check((await source()).includes('"This quarter reshaped the business."'), "Use it makes the text read so");
  check((await status()).startsWith("Tightened “This quarter reshaped the business.”") && (await status()).includes("undoes it"), `and says so: ${await status()}`);
  await page.focus("#tab-inspector");
  await page.keyboard.press("Control+z");
  check(await page.waitForFunction((o) => window.scaena.source() === o, original, { timeout: 10000 }).then(() => true, () => false), "one undo puts the words back");

  // 3. An answer edited in the dialog is taken as edited.
  await tool("proofread", "subtitle");
  check(await answered("#writing-answer"), "Proofread offers an answer");
  await page.fill("#writing-answer", "This quarter changed the business’s shape.");
  await page.click("#writing-use");
  await page.evaluate(() => window.scaena.writing.settled());
  check((await source()).includes("This quarter changed the business’s shape."), "an answer edited is taken as edited");
  await page.focus("#tab-inspector");
  await page.keyboard.press("Control+z");
  await page.waitForFunction((o) => window.scaena.source() === o, original, { timeout: 10000 }).catch(() => {});

  // 4. Leave it changes nothing. An answer emptied in the dialog is nothing to take, as no answer is.
  await tool("rewrite", "subtitle");
  check(await answered("#writing-answer"), "Rewrite offers an answer");
  await page.fill("#writing-answer", " \n ");
  check(await page.locator("#writing-use").isDisabled(), "an answer emptied in the dialog offers nothing to take");
  await page.fill("#writing-answer", "This quarter changed the business.");
  check(await page.locator("#writing-use").isEnabled(), "and one with words again does");
  await page.click("#writing-leave");
  await page.evaluate(() => window.scaena.writing.settled());
  check((await source()) === original && !(await dialog()).open, "Leave it changes nothing");

  // 5. Draft Notes, with nothing selected: the slide's words and its beat's claim; its notes written.
  await tool("notes", undefined);
  check(await answered("#writing-answer"), "Draft Notes offers notes");
  const noted = scripted.requests.at(-1)?.body.messages?.[0]?.content?.[0]?.text ?? "";
  check(noted.startsWith("The slide reads:\n") && noted.includes("Q3 Review") && noted.includes("The point it makes:"), `asked from the slide's words and its claim: ${JSON.stringify(noted)}`);
  await page.click("#writing-use");
  await page.evaluate(() => window.scaena.writing.settled());
  check((await source()).includes("Open on the quarter: the business changed shape"), "Use it writes the slide's notes");
  check((await status()).startsWith("Drafted the notes of Slide 1"), `and says so: ${await status()}`);
  await page.focus("#tab-inspector");
  await page.keyboard.press("Control+z");
  await page.waitForFunction((o) => window.scaena.source() === o, original, { timeout: 10000 }).catch(() => {});

  // 5b. No answer from the model: nothing is offered to take, and it says so.
  await tool("rewrite", "title");
  await page.waitForFunction(() => /No answer/.test(document.querySelector("#writing-thinking").textContent), null, { timeout: 30000 }).catch(() => {});
  const silent = await dialog();
  check(/^No answer from Anthropic/.test(silent.thinking ?? "") && !silent.use && silent.answer === undefined, `no answer offers nothing to take: ${silent.thinking}`);
  await page.click("#writing-leave");
  check((await source()) === original, "and changes nothing");

  // 6. Explain, on a finding: what it means, only read.
  const long = "Q3 Review: the quarter that changed the shape of the business for good";
  // The slide's title, not the deck's, which the header names first.
  await page.evaluate((t) => window.scaena.type(window.scaena.source().replace('text role:display "Q3 Review"', `text role:display "${t}"`)), long);
  await page.waitForFunction(() => document.querySelector("#problems li button.explain"), null, { timeout: 30000 });
  await page.click("#problems li button.explain");
  check(await answered("#writing-said"), "Explain says what a finding means");
  const explained = await dialog();
  check(explained.said?.startsWith("The title's words run past") && explained.leave === "Done" && !explained.use, `only read: ${explained.said}`);
  const why = scripted.requests.at(-1)?.body.messages?.[0]?.content?.[0]?.text ?? "";
  check(/^Finding [EW]\d+ \((error|warning)\): /.test(why), `asked as the finding says it: ${JSON.stringify(why.split("\n")[0])}`);
  await page.click("#writing-leave");
  check(scripted.requests.length === 6, `one request for each tool asked: ${scripted.requests.length}`);

  // 6b. The source's own issue, while it does not compile, is explained all the same.
  await page.evaluate((s) => window.scaena.type(`${s}\nthis is not scn {`), await source());
  await page.waitForFunction(() => window.scaena.last()?.error && document.querySelector("#problems li button.explain"), null, { timeout: 30000 }).catch(() => {});
  await page.click("#problems li button.explain");
  check(await answered("#writing-said"), "Explain says what the source's own issue means");
  const compiling = await dialog();
  check(compiling.title === "About This Issue" && scripted.requests.length === 7, `asked once, though the deck does not compile: ${compiling.title} · ${await status()}`);
  await page.click("#writing-leave");

  // 7. Describe (PLAN 2.119): the picture as the slide shows it goes before the words.
  await page.goto(`${site.origin}/web/dist/editor.html?painter=cpu&bundle=/tests/fixtures/torture.scaena/deck.json`);
  await page.waitForFunction(() => window.scaena?.last(), null, { timeout: 120000 });
  if ((await page.inputValue("#key").catch(() => "")) !== "writing-test-key") {
    await page.selectOption("#provider", "anthropic");
    await page.fill("#base", scripted.origin);
    await page.fill("#key", "writing-test-key");
    await page.waitForFunction(() => document.querySelector("#model").options.length > 0, null, { timeout: 30000 });
  }
  const index = await page.evaluate(() => window.scaena.opened.states.indexOf("adjust"));
  const at = (await source()).indexOf("state adjust") + "state ".length;
  await page.evaluate((offset) => window.scaena.cursor(offset), at);
  await page.waitForFunction((i) => window.scaena.shown() === i, index, { timeout: 30000 });
  await page.waitForFunction(() => window.scaena.canvas.boxed() === "adjust" && window.scaena.canvas.boxes().length > 0, null, { timeout: 30000 });
  const tortured = await source();
  await tool("describe", "adj-mono");
  check(await answered("#writing-answer"), "Describe offers a description");
  const described = await dialog();
  check(described.title === "Description" && described.was === "The same picture in black and white.", `titled, with the description it has: ${described.title} · ${described.was}`);
  check(described.answer === "A gray gradient, a dark block, and a lighter disc.", `the answer, its quotes off: ${described.answer}`);
  const shown = scripted.requests.at(-1)?.body.messages?.[0]?.content ?? [];
  check(shown[0]?.type === "image" && shown[0]?.source?.media_type === "image/png", `the picture goes first, a PNG: ${shown[0]?.type}`);
  check(shown[1]?.text?.startsWith("Describe this picture. The slide it is on reads:"), `then the words: ${JSON.stringify(shown[1]?.text?.split("\n")[0])}`);
  // As the slide shows it: in black and white.
  const gray = await page.evaluate(async (data) => {
    const bitmap = await createImageBitmap(new Blob([Uint8Array.from(atob(data), (c) => c.charCodeAt(0))], { type: "image/png" }));
    const g = new OffscreenCanvas(bitmap.width, bitmap.height).getContext("2d");
    g.drawImage(bitmap, 0, 0);
    const px = g.getImageData(0, 0, bitmap.width, bitmap.height).data;
    let most = 0;
    for (let i = 0; i < px.length; i += 4) most = Math.max(most, Math.max(px[i], px[i + 1], px[i + 2]) - Math.min(px[i], px[i + 1], px[i + 2]));
    return most;
  }, shown[0]?.source?.data ?? "");
  check(gray <= 2, `the picture as the slide shows it, in black and white: ${gray}`);
  await page.click("#writing-use");
  await page.evaluate(() => window.scaena.writing.settled());
  check((await source()).includes("A gray gradient, a dark block, and a lighter disc."), "Use it writes the picture's description");
  check((await status()).startsWith("Described the picture"), `and says so: ${await status()}`);
  await page.focus("#tab-inspector");
  await page.keyboard.press("Control+z");
  check(await page.waitForFunction((o) => window.scaena.source() === o, tortured, { timeout: 10000 }).then(() => true, () => false), "one undo puts the description back");
} finally {
  await browser.close();
  scripted.close();
  site.close();
}

if (failures.length) {
  console.log(`\n${failures.length} failure(s)`);
  process.exit(1);
}
console.log("\nthe writing tools ask once, offer, and take or leave");

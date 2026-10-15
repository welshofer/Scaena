// PLAN O.4f check: Insert › Haiku… in headless Chromium (serve.mjs), the model played by a scripted
// server that speaks Anthropic's Messages API. On the revenue example (Dusk, which has the Haiku
// layout and the verse role):
//
// - With no key, nothing is asked, and the dialog says where a key goes.
// - About a topic: an answer of 5, 8, and 5 is asked once more with each line's count, and the
//   second, of 5, 7, and 5, offered, each line's count beside it; Use It puts it on a new Haiku slide
//   after this one, one patch and one undo step; the slide's notes name the model, and nothing on
//   the slide does; the attribution waits empty.
// - The whole talk: the deck's words go inside <talk> tags.
// - This slide: on the new Haiku slide, with its verse selected, where it goes is not asked, and the
//   poem replaces the verse's words.
// - Write Another names the poem not to repeat.
// - A line edited is counted again, and one off its count stands out.
// - axe-core finds nothing against WCAG 2.1 AA in the dialog.
//
//   node web/haiku.mjs     (after `just web`; from the repository's root)
//
// Exits 1 on any failure.
import { createServer } from "node:http";
import { createRequire } from "node:module";
import { launch, serve } from "./serve.mjs";

const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};
const axe = createRequire(import.meta.url).resolve("axe-core/axe.min.js");

const NEAR = "An old silent pond\nA green frog jumps into the pond\nSplash! Silence again";
const KEPT = "An old silent pond\nA frog jumps into the pond\nSplash! Silence again";
const OTHER = "Numbers climb at dawn\nthe quarter doubles its reach\nwe count, and we breathe";

/** Anthropic's Messages API, scripted: the first haiku asked is 5, 8, and 5; a second ask, once
 * more, gets 5, 7, and 5; Write Another gets another. */
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
    let text = "";
    for await (const chunk of req) text += chunk;
    const body = JSON.parse(text);
    requests.push({ headers: req.headers, body });
    const asked = body.messages?.[0]?.content?.find?.((c) => c.type === "text")?.text ?? "";
    const answer = asked.includes("Write a different haiku") ? OTHER : asked.includes("Its lines count") ? KEPT : NEAR;
    reply(200, { content: [{ type: "text", text: answer }], stop_reason: "end_turn", usage: { input_tokens: 80, output_tokens: 20 } });
  }).listen(0, "127.0.0.1");
  await new Promise((ok) => server.once("listening", ok));
  return { origin: `http://127.0.0.1:${server.address().port}`, requests, close: () => server.close() };
}

let states = 0;
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
  const insert = async () => {
    await page.selectOption("#insert", "haiku");
    await page.waitForFunction(() => document.querySelector("#haiku").open, null, { timeout: 10000 });
  };
  const offered = () => page.waitForFunction(() => !document.querySelector("#haiku-poem").hidden, null, { timeout: 30000 }).then(() => true, () => false);
  const lines = () => page.evaluate(() => [0, 1, 2].map((i) => document.querySelector(`#haiku-line-${i}`).value));
  const counts = () => page.evaluate(() => [0, 1, 2].map((i) => document.querySelector(`#haiku-count-${i}`).textContent));
  const settled = () => page.evaluate(() => window.scaena.haiku.settled());

  // 1. No key: nothing asked, and the dialog says where one goes.
  await insert();
  await page.fill("#haiku-topic", "a quarter that doubled");
  await page.click("#haiku-write");
  await settled();
  check(!(await page.locator("#haiku-key").isHidden()), "with no key, it offers to add one");
  check(/key/.test(await page.textContent("#haiku-status")), `and says where one goes: ${await page.textContent("#haiku-status")}`);
  check(scripted.requests.length === 0, "and asks nothing");
  await page.click("#haiku-key");

  await page.selectOption("#provider", "anthropic");
  await page.fill("#base", scripted.origin);
  await page.fill("#key", "haiku-test-key");
  await page.waitForFunction(() => document.querySelector("#model").options.length > 0, null, { timeout: 30000 });

  // 2. A topic: asked once more on a near miss, and the second placed on a new slide.
  await insert();
  check(!(await page.locator("#haiku-where").isHidden()), "with nothing selected, it asks where it goes");
  await page.fill("#haiku-topic", "a quarter that doubled");
  await page.click("#haiku-write");
  await settled();
  check(await offered(), "a haiku is offered");
  check(scripted.requests.length === 2, `a near miss is asked once more: ${scripted.requests.length} requests`);
  const [first, second] = scripted.requests;
  check(first?.body.messages[0].content[0].text === "Write a haiku about: a quarter that doubled", "asked about the topic");
  check(first?.body.system?.[0]?.text.startsWith("You write one haiku for a slide of a talk, in English"), "told the engine's instructions");
  check(second?.body.messages[0].content[0].text.includes("Its lines count 5, 8, and 5 syllables"), "the second ask says each line's count");
  check(first?.headers["x-api-key"] === "haiku-test-key", "the key goes to the provider in its own header");
  check(JSON.stringify(await lines()) === JSON.stringify(KEPT.split("\n")), `the second offered: ${await lines()}`);
  check(JSON.stringify(await counts()) === JSON.stringify(["5", "7", "5"]), `each line's count beside it: ${await counts()}`);
  await page.evaluate(async (file) => {
    if (!window.axe) await import(file);
  }, `${site.origin}/web/node_modules/axe-core/axe.min.js`).catch(() => {});
  await page.addScriptTag({ path: axe });
  const found = await page.evaluate(async () => (await window.axe.run("#haiku", { runOnly: ["wcag2a", "wcag2aa", "wcag21aa"] })).violations.map((v) => v.id));
  check(found.length === 0, `axe-core finds nothing against WCAG 2.1 AA in the dialog: ${found}`);
  // A line edited is counted again, and one off its count stands out.
  await page.fill("#haiku-line-1", "A green frog jumps into the pond");
  await page.waitForFunction(() => document.querySelector("#haiku-count-1").textContent === "8", null, { timeout: 10000 }).catch(() => {});
  check((await counts())[1] === "8" && (await page.locator("#haiku-line-1.off").count()) === 1, `an edit is counted again, off its count: ${await counts()}`);
  await page.fill("#haiku-line-1", "A frog jumps into the pond");
  states = (await page.evaluate(() => window.scaena.last().states.length));
  await page.click("#haiku-use");
  await settled();
  const placed = await source();
  check(placed.includes("layout:haiku") || placed.includes('layout: haiku') || placed.includes("haiku"), "Use It starts a Haiku slide");
  check((await page.evaluate(() => window.scaena.last().states.length)) === states + 1, "one slide more");
  check(placed.includes("Haiku written by scripted with your Anthropic key on "), "the notes name the model");
  check(!/text[^\n]*scripted/.test(placed.replace(/notes[^\n]*/g, "")), "and nothing on the slide does");
  check(!placed.includes("Who wrote it, or what it is on"), "the attribution waits empty");
  check(/Placed a haiku on a new slide/.test(await status()), `the status says so: ${await status()}`);
  await page.focus("#tab-inspector");
  await page.keyboard.press("Control+z");
  check(await page.waitForFunction((o) => window.scaena.source() === o, original, { timeout: 10000 }).then(() => true, () => false), "one undo takes it back");
  await page.keyboard.press("Control+Shift+z");
  await page.waitForFunction((p) => window.scaena.source() === p, placed, { timeout: 10000 }).catch(() => {});

  // 3. The whole talk, inside <talk> tags.
  await insert();
  await page.check('input[name="haiku-about"][value="talk"]');
  await page.click("#haiku-write");
  await settled();
  const talk = scripted.requests.at(-2)?.body.messages[0].content[0].text ?? "";
  check(talk.includes("<talk>\n") && talk.includes("</talk>"), "the whole talk goes inside <talk> tags");
  // 4. Write Another names the poem not to repeat.
  check(await offered(), "the talk's haiku is offered");
  await page.click("#haiku-another");
  await settled();
  const another = scripted.requests.at(-1)?.body.messages[0].content[0].text ?? "";
  check(another.includes("Write a different haiku from this one:\nAn old silent pond"), "Write Another names the poem not to repeat");
  check(JSON.stringify(await lines()) === JSON.stringify(OTHER.split("\n")), `and offers another: ${await lines()}`);
  await page.click("#haiku-leave");
  await settled();

  // 5. This slide: on the Haiku slide, its verse selected, the poem replaces the verse's words.
  const text = await source();
  const haikuSlide = text.match(/^state (\S+) [^\n]*layout:haiku/m)?.[1] ?? "";
  const index = await page.evaluate((s) => window.scaena.last().states.findIndex((t) => (t.id ?? t) === s), haikuSlide);
  await page.evaluate((offset) => window.scaena.cursor(offset), text.indexOf(`state ${haikuSlide}`) + "state ".length);
  await page.waitForFunction((i) => window.scaena.shown() === i, index, { timeout: 30000 }).catch(() => {});
  await page.waitForFunction((s) => window.scaena.canvas.boxed() === s && window.scaena.canvas.boxes().length > 0, haikuSlide, { timeout: 30000 }).catch(() => {});
  const verse = await page.evaluate(() => window.scaena.canvas.boxes().find((b) => /poem/.test(b.node))?.node);
  check(verse !== undefined, `the Haiku slide ${haikuSlide} shows its verse`);
  if (verse) await page.evaluate((n) => window.scaena.canvas.select(n), verse);
  await insert();
  check(await page.locator("#haiku-where").isHidden(), "with a verse selected, where it goes is not asked");
  await page.check('input[name="haiku-about"][value="slide"]');
  await page.click("#haiku-write");
  await settled();
  check(await offered(), "this slide's haiku is offered");
  await page.fill("#haiku-line-0", "A still silent pond");
  await page.click("#haiku-use");
  await settled();
  check((await source()).includes("A still silent pond") && (await page.evaluate(() => window.scaena.last().states.length)) === states + 1, "the verse takes the poem, and no slide is added");
} catch (e) {
  failures.push(`error: ${e.message}`);
} finally {
  await browser.close();
  site.close();
  scripted.close();
}
if (failures.length) {
  console.log(`\n${failures.length} failed`);
  process.exit(1);
}

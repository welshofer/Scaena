// PLAN 2.7 site check: the static site `just site` builds (target/site), served as a host
// would serve it, from a path under its origin and with each file's media type, in headless
// Chromium. The CPU paints, as in web/player.mjs.
//
//   node web/site.mjs     (after `just site`; from the repository's root)
//
// - The site's address alone plays its demo deck: each of the deck's states, shown, paints a
//   frame that is not blank.
// - Edit opens the editor on the same deck, which compiles and lints with no error; Play opens
//   the player on it again.
// - What the pages load only when asked comes from the site too: the subsetter, to download
//   the deck; the assistant's code and what it reads, to answer a question, here of a
//   scripted server that answers as Anthropic's API does.
// - Nothing is asked of another origin but the assistant's provider, nothing outside the
//   site's path, and every file asked of the site is there.
// Exits 1 on any failure.
import { readdir, readFile } from "node:fs/promises";
import { createServer } from "node:http";
import { join } from "node:path";
import { decode, launch, serve } from "./serve.mjs";

const root = join(process.cwd(), "target/site");
const at = "/scaena/demo/";
const media = { ".css": "text/css", ".png": "image/png", ".ttf": "font/ttf", ".csv": "text/csv", ".txt": "text/plain", ".md": "text/markdown" };
const site = await serve(root, { at, more: media });
const home = `${site.origin}${at}`;
const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};

const decks = await readdir(join(root, "decks"));
check(decks.length === 1, `the site holds one demo deck: ${decks.join(", ")}`);
const deck = JSON.parse(await readFile(join(root, "decks", decks[0], "deck.json"), "utf8"));
const states = deck.states.map((s) => s.id);

/** A scripted provider on an origin of its own, answering as Anthropic's API does, with a
 * word and no call: each request it took. */
async function provider() {
  const requests = [];
  const server = createServer(async (req, res) => {
    const cors = {
      "access-control-allow-origin": req.headers.origin ?? "*",
      "access-control-allow-headers": req.headers["access-control-request-headers"] ?? "*",
      "access-control-allow-methods": "GET, POST, OPTIONS",
    };
    if (req.method === "OPTIONS") return res.writeHead(204, cors).end();
    const reply = (body) => res.writeHead(200, { ...cors, "content-type": "application/json" }).end(JSON.stringify(body));
    if (req.method === "GET") return reply({ data: [{ id: "scripted", type: "model" }] });
    let text = "";
    for await (const chunk of req) text += chunk;
    requests.push(JSON.parse(text));
    reply({ content: [{ type: "text", text: "A season of trail work." }], stop_reason: "end_turn", usage: { input_tokens: 900, output_tokens: 6 } });
  }).listen(0, "127.0.0.1");
  await new Promise((ok) => server.once("listening", ok));
  return { origin: `http://127.0.0.1:${server.address().port}`, requests, close: () => server.close() };
}

const scripted = await provider();
const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1920, height: 1200 }, deviceScaleFactor: 1, acceptDownloads: true });
  // Every request the pages and their workers make, and each one the site could not answer.
  const asked = [];
  const missing = [];
  context.on("request", (r) => asked.push(r.url()));
  context.on("response", (r) => r.url().startsWith(home) && r.status() >= 400 && missing.push(`${r.status()} ${r.url()}`));
  const watch = (page) => {
    page.on("pageerror", (e) => failures.push(`${page.url()}: ${e.message}`));
    page.on("console", (m) => m.type() === "error" && failures.push(`${page.url()}: console: ${m.text()}`));
    return page;
  };
  /** Once `page` has opened its deck, or said why not. */
  const opened = async (page, test) => {
    await page.waitForFunction(test, null, { timeout: 120000 }).catch(() => {});
    return page.evaluate(() => (document.querySelector("#status")?.textContent ?? "").startsWith("error") && document.querySelector("#status").textContent);
  };

  // The player, at the site's address alone (the CPU painter asked for, as in the player's check).
  const player = watch(await context.newPage());
  await player.goto(`${home}?painter=cpu`);
  const error = await opened(player, () => window.scaena?.at || document.querySelector("#status")?.textContent.startsWith("error"));
  check(!error, `the player opens the demo deck${error ? `: ${error}` : ""}`);
  const shown = await player.evaluate(() => window.scaena?.states ?? []);
  check(JSON.stringify(shown) === JSON.stringify(states), `it plays ${decks[0]}'s ${states.length} states: ${shown.join(", ")}`);
  let blank = [];
  for (const state of shown) {
    await player.evaluate((s) => window.scaena.show(s), state);
    const { rgba } = decode(await player.locator("#stage").screenshot());
    const colors = new Set();
    for (let i = 0; i < rgba.length && colors.size <= 16; i += 4 * 97) colors.add(rgba.readUInt32BE(i));
    if (colors.size <= 16) blank.push(`${state} (${colors.size} colors)`);
  }
  check(shown.length > 0 && !blank.length, `each state paints a frame that is not blank${blank.length ? `: ${blank.join(", ")}` : ""}`);

  // Edit: the editor on the same deck.
  await player.click("#edit");
  const editor = player;
  const editing = await opened(editor, () => window.scaena?.last?.() || document.querySelector("#status")?.textContent.startsWith("error"));
  check(!editing && editor.url().startsWith(`${home}editor.html?`), `Edit opens the editor: ${editor.url().slice(site.origin.length)}${editing ? `: ${editing}` : ""}`);
  await editor.waitForFunction(() => window.scaena.wholes().length > 0 && window.scaena.last().whole, null, { timeout: 60000 }).catch(() => {});
  const linted = await editor.evaluate(() => window.scaena.last());
  const errors = linted?.findings.filter((f) => f.severity === "error") ?? [];
  check(linted?.valid && linted.whole && !errors.length, `it compiles ${decks[0]} and lints every state with no error (${errors.map((f) => f.code).join(", ") || "none"})`);
  const play = await editor.$eval("#play", (a) => (a.hidden ? "" : a.href));
  check(play.startsWith(`${home}index.html?`) && new URL(play).searchParams.get("bundle"), `Play is the player on it: ${play.slice(site.origin.length)}`);
  const [again] = await Promise.all([context.waitForEvent("page"), editor.click("#play")]);
  watch(again);
  const back = await opened(again, () => window.scaena?.at || document.querySelector("#status")?.textContent.startsWith("error"));
  const replayed = await again.evaluate(() => window.scaena?.states ?? []);
  check(!back && JSON.stringify(replayed) === JSON.stringify(states), `Play opens the player on the deck again${back ? `: ${back}` : ""}`);
  await again.close();

  // What loads when asked: the subsetter, to download.
  const [download] = await Promise.all([editor.waitForEvent("download"), editor.click("#download")]);
  const zip = await readFile(await download.path());
  check(zip[0] === 0x50 && zip[1] === 0x4b, `Download loads the subsetter from the site: ${download.suggestedFilename()}, ${zip.length} bytes`);

  // And the assistant's code and what it reads, to answer.
  await editor.click("#tab-assistant");
  await editor.selectOption("#provider", "anthropic");
  await editor.fill("#base", scripted.origin);
  await editor.fill("#key", "site-test-key");
  await editor.waitForFunction(() => document.querySelector("#model").options.length > 0, null, { timeout: 30000 }).catch(() => {});
  const done = await editor.evaluate(() => window.scaena.assistant.ask("What is this deck about?"));
  const system = scripted.requests[0]?.system?.[0]?.text ?? "";
  check(done.kind === "done", `the assistant answers (${JSON.stringify(done)})`);
  check(system.includes("scaena://spec") && system.includes("--- scaena://skills/author-deck ---") && system.includes(deck.meta.title), "its code and what it reads load from the site: the resources, the author-deck skill, and the deck");

  // Nothing from elsewhere, and nothing missing.
  const outside = asked.filter((u) => !u.startsWith(home) && !u.startsWith(`${scripted.origin}/`) && !/^(blob|data):/.test(u));
  check(!outside.length, `nothing is asked of anywhere but the site and the provider${outside.length ? `: ${[...new Set(outside)].join(", ")}` : ""}`);
  check(!missing.length, `every file asked of the site is there${missing.length ? `: ${missing.join(", ")}` : ""}`);
  const loaded = (pattern) => asked.some((u) => u.startsWith(home) && pattern.test(u));
  check(
    [/\/assets\/scaena_wasm_bg-[^/]+\.wasm$/, /\/assets\/scaena_subset_bg-[^/]+\.wasm$/, /\/assets\/scaena_resources_bg-[^/]+\.wasm$/, /\/assets\/assistant-[^/]+\.js$/, /\/decks\/[^/]+\/manifest\.json$/].every(loaded),
    `the engine, the subsetter, what the assistant reads, its code, and the deck's manifest came from the site (${asked.filter((u) => u.startsWith(home)).length} requests)`,
  );
} finally {
  await browser.close();
  site.close();
  scripted.close();
}

if (failures.length) {
  console.error(`\n${failures.length} failure(s):\n${failures.join("\n")}`);
  process.exit(1);
}
console.log("\nthe site plays, edits, and asks from its own path");

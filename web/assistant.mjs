// PLAN 2.6 assistant check: the editor's assistant in headless Chromium (serve.mjs), each
// provider played by a scripted server that speaks its wire format: Anthropic's Messages API,
// OpenAI's Chat Completions, and Gemini's generateContent. The script is PLAN 1.19's agent loop,
// which gate 2 asks the assistant to run in the browser (PLAN §2, criterion 4):
//
//   a new deck from Dusk → a headline too long for its one-row slot (deck_patch) → the E100
//   lint finds, with its fix (deck_lint) → the fix (deck_lint with `fix`) → no errors
//   (deck_lint) → the state drawn (deck_render) → a cooler accent (theme_edit) → an answer.
//
//   node web/assistant.mjs     (after `just web`; from the repository's root)
//
// For each provider it checks what the browser sends: the key in that provider's header (and
// Anthropic's opt-in header for a browser), the system prompt with the author-deck skill, the
// tools as the MCP server's less `bundle`, each call's result as the model reads it, and the
// frame as an image; that Gemini's thought signatures go back as they came; that the editor's
// source takes each edit and lints clean at the end; that the theme's edit is one step of the
// source's undo, the theme written back (PLAN 2.61); and that a question stops when asked to.
// Then the key's storage: for the tab, or on the device, encrypted, and forgotten.
// Exits 1 on any failure.
import { copyFile, mkdir, readFile, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { dirname, join } from "node:path";
import { decode, launch, serve } from "./serve.mjs";

const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};

// A new deck from Dusk, as deck_create makes one: the theme, its fonts (each family's own, and
// its italic's), and one empty state.
const bundle = "target/web-assistant/loop";
const theme = JSON.parse(await readFile("docs/examples/themes/dusk.theme.json", "utf8"));
const fonts = [];
for (const family of Object.values(theme.type.families)) {
  for (const [face, style] of [[family, undefined], [family.italic, "italic"]]) {
    if (!face) continue;
    await mkdir(join(bundle, dirname(face.file)), { recursive: true });
    await copyFile(join("docs/examples", face.file), join(bundle, face.file));
    fonts.push({ family: family.family, file: face.file, ...(style && { style }), ...(face.axes && { axes: face.axes }) });
  }
}
await mkdir(join(bundle, "themes"), { recursive: true });
await copyFile("docs/examples/themes/dusk.theme.json", join(bundle, "themes/dusk.theme.json"));
const version = JSON.parse(await readFile("docs/examples/revenue.deck.json", "utf8")).scaena;
const deck = {
  scaena: version,
  meta: { title: "Loop" },
  canvas: { width: 1920, height: 1080 },
  theme: "themes/dusk.theme.json",
  fonts,
  nodes: {},
  states: [{ id: "start" }],
};
await writeFile(join(bundle, "deck.json"), JSON.stringify(deck, null, 2));

const HEADLINE = "Volunteers rebuilt fifty-two miles of trail";
/** The agent loop, a call a turn, then the answer. */
const SCRIPT = [
  {
    name: "deck_patch",
    args: {
      ops: [
        { op: "add", path: "/states/0/layout", value: "figure" },
        {
          op: "add_node",
          id: "headline",
          state: "start",
          node: { type: "text", role: "headline", semantic: "claim", at: { in: "header" }, text: HEADLINE },
        },
      ],
    },
  },
  { name: "deck_lint", args: { severity: "error" } },
  { name: "deck_lint", args: { fix: true } },
  { name: "deck_lint", args: {} },
  { name: "deck_render", args: { state: "start", size: "960x540" } },
  { name: "theme_edit", args: { ops: [{ op: "replace", path: "/tokens/color/accent", value: "#2E86E4" }] } },
];
const ANSWER = "The headline fits its slot now, and the deck lints clean.";

/** Each provider's wire format: how many answers a request holds already, the results of the
 * calls it answers, what to answer, and where its key goes. */
const WIRE = {
  anthropic: {
    models: { path: "/v1/models", body: { data: [{ id: "scripted", type: "model" }] } },
    turn: "/v1/messages",
    answered: (b) => b.messages.filter((m) => m.role === "assistant").length,
    results: (b) =>
      (b.messages.at(-1)?.content ?? [])
        .filter((c) => c.type === "tool_result")
        .map((c) => ({
          json: c.content.find((p) => p.type === "text")?.text,
          error: c.is_error,
          png: c.content.find((p) => p.type === "image")?.source?.data,
        })),
    call: (step, i) => ({
      content: [{ type: "tool_use", id: `toolu_${i}`, name: step.name, input: step.args }],
      stop_reason: "tool_use",
      usage: { input_tokens: 1000, output_tokens: 50 },
    }),
    answer: () => ({ content: [{ type: "text", text: ANSWER }], stop_reason: "end_turn", usage: { input_tokens: 1200, output_tokens: 20 } }),
    key: (h) => h["x-api-key"],
    system: (b) => b.system?.[0]?.text ?? "",
    tools: (b) => b.tools.map((t) => ({ name: t.name, schema: t.input_schema })),
  },
  openai: {
    models: { path: "/v1/models", body: { data: [{ id: "scripted", object: "model" }] } },
    turn: "/v1/chat/completions",
    answered: (b) => b.messages.filter((m) => m.role === "assistant").length,
    results: (b) => {
      const last = b.messages.findLastIndex((m) => m.role === "assistant");
      const after = b.messages.slice(last + 1);
      const frames = after.filter((m) => m.role === "user").flatMap((m) => (Array.isArray(m.content) ? m.content : []));
      const pngs = frames.filter((p) => p.type === "image_url").map((p) => p.image_url.url.replace(/^data:image\/png;base64,/, ""));
      return after.filter((m) => m.role === "tool").map((m, i) => ({ json: m.content, png: pngs[i] }));
    },
    call: (step, i) => ({
      choices: [
        {
          message: { role: "assistant", content: null, tool_calls: [{ id: `call_${i}`, type: "function", function: { name: step.name, arguments: JSON.stringify(step.args) } }] },
          finish_reason: "tool_calls",
        },
      ],
      usage: { prompt_tokens: 1000, completion_tokens: 50 },
    }),
    answer: () => ({ choices: [{ message: { role: "assistant", content: ANSWER }, finish_reason: "stop" }], usage: { prompt_tokens: 1200, completion_tokens: 20 } }),
    key: (h) => h.authorization?.replace(/^Bearer /, ""),
    system: (b) => b.messages.find((m) => m.role === "system")?.content ?? "",
    tools: (b) => b.tools.map((t) => ({ name: t.function.name, schema: t.function.parameters })),
  },
  gemini: {
    models: { path: "/v1beta/models", body: { models: [{ name: "models/scripted", supportedGenerationMethods: ["generateContent"] }] } },
    turn: "/v1beta/models/scripted:generateContent",
    answered: (b) => b.contents.filter((c) => c.role === "model").length,
    results: (b) => {
      const parts = b.contents.at(-1)?.parts ?? [];
      const pngs = parts.filter((p) => p.inlineData).map((p) => p.inlineData.data);
      return parts
        .filter((p) => p.functionResponse)
        .map((p, i) => {
          const r = p.functionResponse.response;
          return { json: JSON.stringify(r.result ?? r.error), error: "error" in r, png: pngs[i] };
        });
    },
    call: (step, i) => ({
      candidates: [{ content: { role: "model", parts: [{ functionCall: { name: step.name, args: step.args }, thoughtSignature: `signature-${i}` }] }, finishReason: "STOP" }],
      usageMetadata: { promptTokenCount: 1000, candidatesTokenCount: 50 },
    }),
    answer: () => ({ candidates: [{ content: { role: "model", parts: [{ text: ANSWER }] }, finishReason: "STOP" }], usageMetadata: { promptTokenCount: 1200, candidatesTokenCount: 20 } }),
    key: (h) => h["x-goog-api-key"],
    system: (b) => b.systemInstruction?.parts?.[0]?.text ?? "",
    tools: (b) => b.tools[0].functionDeclarations.map((t) => ({ name: t.name, schema: t.parametersJsonSchema })),
  },
};

/** A scripted provider on a port of its own, answering as the browser asks across origins:
 * each request it took, and `slow`, to answer only after a while. */
async function provider(wire) {
  const requests = [];
  const state = { slow: 0 };
  const server = createServer(async (req, res) => {
    const cors = {
      "access-control-allow-origin": req.headers.origin ?? "*",
      "access-control-allow-headers": req.headers["access-control-request-headers"] ?? "*",
      "access-control-allow-methods": "GET, POST, OPTIONS",
    };
    if (req.method === "OPTIONS") return res.writeHead(204, cors).end();
    const path = new URL(req.url, "http://x").pathname;
    const reply = (status, body) => res.writeHead(status, { ...cors, "content-type": "application/json" }).end(JSON.stringify(body));
    if (req.method === "GET" && path === wire.models.path) return reply(200, wire.models.body);
    if (req.method !== "POST" || path !== wire.turn) return reply(404, { error: { message: `no ${req.method} ${path}` } });
    let text = "";
    for await (const chunk of req) text += chunk;
    const body = JSON.parse(text);
    requests.push({ headers: req.headers, body });
    if (state.slow) await new Promise((ok) => setTimeout(ok, state.slow));
    const i = wire.answered(body);
    reply(200, i < SCRIPT.length ? wire.call(SCRIPT[i], i) : wire.answer());
  }).listen(0, "127.0.0.1");
  await new Promise((ok) => server.once("listening", ok));
  return { origin: `http://127.0.0.1:${server.address().port}`, requests, state, close: () => server.close() };
}

const site = await serve();
const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1 });
  const open = async () => {
    const page = await context.newPage();
    page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
    page.on("console", (m) => m.type() === "error" && !/Failed to load resource/.test(m.text()) && failures.push(`console: ${m.text()}`));
    await page.goto(`${site.origin}/web/dist/editor.html?painter=cpu&source=open&bundle=/${bundle}/deck.json`);
    await page.waitForFunction(() => window.scaena?.last(), null, { timeout: 120000 });
    await page.click("#tab-assistant");
    return page;
  };
  /** Pick `provider` at `origin` with `key`, and wait for its models. */
  const choose = async (page, id, origin, key) => {
    await page.selectOption("#provider", id);
    await page.fill("#base", origin);
    await page.fill("#key", key);
    await page.waitForFunction(() => document.querySelector("#model").options.length > 0, null, { timeout: 30000 });
    return page.$eval("#model", (s) => s.value);
  };
  /** Once the editor has linted the source as it stands. */
  const settled = (page) =>
    page.waitForFunction(
      () => {
        const last = window.scaena.last();
        return last && last.whole && window.scaena.source().length > 0 && !document.querySelector("#code .cm-content[contenteditable=false]");
      },
      null,
      { timeout: 60000, polling: 100 },
    );

  for (const id of ["anthropic", "openai", "gemini"]) {
    const wire = WIRE[id];
    const scripted = await provider(wire);
    const page = await open();
    const before = await page.evaluate(() => window.scaena.source());
    const model = await choose(page, id, scripted.origin, `${id}-test-key`);
    check(model === "scripted", `${id}: the models the key can use are listed (${model})`);
    const done = await page.evaluate(() => window.scaena.assistant.ask("Put the trail headline on the slide, fix what lint finds, and show me."));
    check(done.kind === "done" && done.stop === "end", `${id}: the question is answered (${JSON.stringify(done)})`);
    const { requests } = scripted;
    check(requests.length === SCRIPT.length + 1, `${id}: one request a turn: ${requests.length}`);

    const first = requests[0];
    check(wire.key(first.headers) === `${id}-test-key`, `${id}: the key goes in the provider's own header`);
    if (id === "anthropic") {
      check(first.headers["anthropic-dangerous-direct-browser-access"] === "true" && first.headers["anthropic-version"], "anthropic: the browser's opt-in header and the API version");
      check(first.body.system[0].cache_control?.type === "ephemeral", "anthropic: the system prompt is cached");
    }
    const system = wire.system(first.body);
    check(system.includes("# author-deck") && system.includes("scaena://spec"), `${id}: the system prompt holds the author-deck skill and the resources`);
    const tools = wire.tools(first.body);
    const names = tools.map((t) => t.name);
    check(
      ["deck_read", "deck_patch", "deck_lint", "deck_inspect", "deck_diff", "deck_render", "spine_read", "spine_update", "data_attach", "data_edit", "theme_edit", "resource_read"].every((n) => names.includes(n)),
      `${id}: the tools: ${names.join(", ")}`,
    );
    check(
      tools.every((t) => !t.schema.properties?.bundle && !t.schema.properties?.out && !JSON.stringify(t.schema).includes("$ref")),
      `${id}: no tool takes a bundle or an output path, and every schema is whole`,
    );

    const result = (n) => wire.results(requests[n].body)[0] ?? {};
    const parsed = (n) => JSON.parse(result(n).json ?? "{}");
    const patched = parsed(1);
    check(patched.applied === true && patched.added?.some((f) => f.code === "E100"), `${id}: deck_patch applied, and lint finds an E100 it did not (${result(1).json?.slice(0, 160)})`);
    const linted = parsed(2);
    check(linted.findings?.[0]?.code === "E100" && Array.isArray(linted.findings[0].fix), `${id}: deck_lint finds the E100, with its fix`);
    const fixed = parsed(3);
    check(fixed.fixed?.some((f) => f.code === "E100") && fixed.errors === 0, `${id}: deck_lint with fix fixes it: ${fixed.errors} errors`);
    check(parsed(4).errors === 0, `${id}: deck_lint finds no errors after`);
    const frame = result(5);
    const png = frame.png && decode(Buffer.from(frame.png, "base64"));
    check(png?.width === 960 && png?.height === 540, `${id}: deck_render's frame reaches the model as an image, ${png?.width}×${png?.height}`);
    check(JSON.parse(frame.json ?? "{}").digest?.length === 16, `${id}: with the frame's facts`);
    const themed = parsed(6);
    check(
      themed.applied === true && themed.theme === "themes/dusk.theme.json" && themed.paths?.[0] === "/tokens/color/accent",
      `${id}: theme_edit edits the bundle's copy of Dusk (${result(6).json?.slice(0, 160)})`,
    );
    if (id === "gemini") {
      const model = requests[1].body.contents.find((c) => c.role === "model");
      check(model?.parts?.[0]?.thoughtSignature === "signature-0", "gemini: a call's thought signature goes back as it came");
    }

    await settled(page);
    const after = await page.evaluate(() => window.scaena.source());
    check(after !== before && after.includes(HEADLINE), `${id}: the editor's source took each edit`);
    const last = await page.evaluate(() => window.scaena.last());
    check(last.valid && !last.findings.some((f) => f.severity === "error"), `${id}: and lints clean: ${last.findings.map((f) => f.code).join(", ") || "nothing found"}`);
    const transcript = await page.evaluate(() => window.scaena.assistant.transcript());
    check(
      transcript.filter((t) => t.startsWith("call")).length === SCRIPT.length && transcript.at(-1) === `said: ${ANSWER}`,
      `${id}: the conversation shows each call and the answer (${transcript.length} entries)`,
    );
    const shown = await page.$eval("#transcript img", (img) => img.naturalWidth).catch(() => 0);
    check(shown === 960, `${id}: and the frame the model saw`);

    // The theme's edit shows in the Theme tab, and is one step of the source's undo: ⌘Z writes the
    // theme back (PLAN 2.61).
    const accent = () => page.evaluate(() => window.scaena.theme.theme()?.tokens?.color?.accent);
    await page.click("#tab-theming");
    await page.waitForFunction(() => window.scaena.theme.theme()?.tokens?.color?.accent === "#2E86E4", null, { timeout: 30000 }).catch(() => {});
    check((await accent()) === "#2E86E4", `${id}: the Theme tab shows the assistant's edit: ${await accent()}`);
    await page.locator("#code .cm-content").focus();
    await page.keyboard.press("Control+z");
    await page.waitForFunction(() => window.scaena.theme.theme()?.tokens?.color?.accent === "#FF6A3D", null, { timeout: 30000 }).catch(() => {});
    check((await accent()) === "#FF6A3D" && (await page.evaluate(() => window.scaena.source())) === after, `${id}: ⌘Z writes the theme back, the source as it was`);
    await page.click("#tab-assistant");
    const usage = await page.evaluate(() => window.scaena.assistant.usage());
    check(usage.input === 1000 * SCRIPT.length + 1200, `${id}: tokens counted: ${usage.input} in, ${usage.output} out`);

    if (id === "anthropic") {
      // A question asked again stops when the user says so, and the source takes typing again.
      scripted.state.slow = 4000;
      await page.evaluate(() => {
        window.asked = window.scaena.assistant.ask("Make it shorter.");
      });
      await page.waitForFunction(() => document.querySelector("#stop") && !document.querySelector("#stop").hidden, null, { timeout: 10000 });
      const locked = await page.$eval("#code .cm-content", (e) => e.getAttribute("contenteditable"));
      check(locked === "false", "the source is read-only while the assistant works");
      const stoppedAt = Date.now();
      await page.click("#stop");
      const stopped = await page.evaluate(() => window.asked);
      check(stopped.kind === "done" && stopped.stop === "stopped" && Date.now() - stoppedAt < 3000, `Stop stops it at once (${JSON.stringify(stopped)})`);
      const editable = await page.$eval("#code .cm-content", (e) => e.getAttribute("contenteditable"));
      check(editable === "true", "and the source takes typing again");
      scripted.state.slow = 0;

      // The key: for this tab by default; on this device, encrypted, when asked; then forgotten.
      const stored = () => page.evaluate(() => ({ session: sessionStorage.getItem("scaena.key.anthropic"), local: localStorage.getItem("scaena.key.anthropic") }));
      let keys = await stored();
      check(keys.session === "anthropic-test-key" && keys.local === null, "a key is kept for the tab by default");
      await page.check("#keep-key");
      await page.waitForFunction(() => localStorage.getItem("scaena.key.anthropic"), null, { timeout: 10000 });
      keys = await stored();
      check(keys.session === null && keys.local && !keys.local.includes("anthropic-test-key"), "kept on this device, it is encrypted");
      await page.reload();
      await page.waitForFunction(() => window.scaena?.last(), null, { timeout: 120000 });
      await page.click("#tab-assistant");
      await page.waitForFunction(() => document.querySelector("#key").value, null, { timeout: 10000 });
      check((await page.$eval("#key", (k) => k.value)) === "anthropic-test-key", "and comes back after a reload");
      await page.click("#forget-key");
      keys = await stored();
      check(keys.session === null && keys.local === null && (await page.$eval("#key", (k) => k.value)) === "", "Forget key forgets it everywhere");
    }
    await page.close();
    scripted.close();
  }
} finally {
  await browser.close();
  site.close();
}

if (failures.length) {
  console.error(`\n${failures.length} failure(s):\n${failures.join("\n")}`);
  process.exit(1);
}
console.log("the assistant runs the agent loop with each provider, stops when asked, and keeps the key as told");

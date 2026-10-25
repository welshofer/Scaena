// PLAN 2.6 assistant check: the editor's assistant in headless Chromium (serve.mjs), each
// provider played by a scripted server that speaks its wire format: Anthropic's Messages API,
// OpenAI's Chat Completions, and Gemini's generateContent. The script is PLAN 1.19's agent loop,
// which gate 2 asks the assistant to run in the browser (PLAN §2, criterion 4):
//
//   a new deck from Dusk → a headline too long for its slot (deck_patch) → the E100
//   lint finds, with its fix (deck_lint) → the fix (deck_lint with `fix`) → no errors
//   (deck_lint) → the state drawn (deck_render) → a cooler accent (theme_edit) → an icon found by
//   its words (icon_find, which the author-deck skill names, from the set the page carries) → a
//   patch the deck refuses (deck_patch), headed as not done → an answer.
//
//   node web/assistant.mjs     (after `just web`; from the repository's root)
//
// For each provider it checks what the browser sends: the key in that provider's header (and
// Anthropic's opt-in header for a browser), the system prompt with the author-deck skill, the
// tools as the MCP server's less `bundle`, each call's result as the model reads it, and the
// frame as an image; that Gemini's thought signatures go back as they came; that the editor's
// source takes each edit and lints clean at the end; that each edit shows the states it drew
// anew, before and after (PLAN 2.93); that the theme's edit is one step of the source's undo, the
// theme written back (PLAN 2.61); and that a question stops when asked to.
// Then the key's field and storage: masked and shown on request, its form said as it is typed, its
// provider's answer said (it works with its models, or is refused, in a person's words), kept
// for the tab, or on the device, encrypted, and forgotten.
// Exits 1 on any failure.
import { copyFile, mkdir, readFile, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { dirname, join } from "node:path";
import { decode, launch, serve, tab } from "./serve.mjs";

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

const HEADLINE = "Volunteers rebuilt fifty-two miles of trail across the state park";
/** The agent loop, a call a turn, then the answer. */
const SCRIPT = [
  {
    name: "deck_patch",
    args: {
      ops: [
        { op: "add", path: "/states/0/layout", value: "agenda-photo" },
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
  { name: "icon_find", args: { query: "rocket", weight: "fill", limit: 2 } },
  // A patch the deck refuses: a layout the theme has not.
  { name: "deck_patch", args: { ops: [{ op: "set_state", id: "start", prop: "layout", value: "no-such-layout" }] } },
  // The deck's story, read by the generator's module (narrative FUNC-2).
  { name: "deck_health", args: {} },
];
/** The answer, markdown as a model writes it (narrative ATTR-4), and how it reads once rendered. */
const ANSWER = "The headline fits its slot now, and the deck lints clean:\n\n- **start** is the slide to look at\n- `start` is linked here\n\nDone.";
const PLAIN = "The headline fits its slot now, and the deck lints clean:start is the slide to look atstart is linked hereDone.";

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
    system: (b) => (b.system ?? []).map((s) => s.text).join(""),
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
    turn: "/v1beta/models/scripted:streamGenerateContent",
    answered: (b) => b.contents.filter((c) => c.role === "model").length,
    results: (b) => {
      const parts = b.contents.at(-1)?.parts ?? [];
      const pngs = parts.filter((p) => p.inlineData).map((p) => p.inlineData.data);
      return parts
        .filter((p) => p.functionResponse)
        .map((p, i) => {
          const r = p.functionResponse.response;
          // A result that is no JSON, as a fenced one is, goes as the string it is.
          const told = r.result ?? r.error;
          return { json: typeof told === "string" ? told : JSON.stringify(told), error: "error" in r, png: pngs[i] };
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

/** `answer`, one of `WIRE`'s, as the provider streams it (narrative ATTR-3): the events a
 * streamed request is answered with, the words of its text each an event of its own, so the
 * page shows them as they come. */
const SSE = {
  anthropic: (answer) => {
    const events = [];
    const event = (type, data) => events.push(`event: ${type}\ndata: ${JSON.stringify({ type, ...data })}\n\n`);
    event("message_start", { message: { id: "msg_scripted", type: "message", role: "assistant", content: [], usage: { input_tokens: answer.usage.input_tokens, output_tokens: 0 } } });
    answer.content.forEach((block, index) => {
      if (block.type === "text") {
        event("content_block_start", { index, content_block: { type: "text", text: "" } });
        for (const piece of words(block.text)) event("content_block_delta", { index, delta: { type: "text_delta", text: piece } });
      } else {
        event("content_block_start", { index, content_block: { type: "tool_use", id: block.id, name: block.name, input: {} } });
        const json = JSON.stringify(block.input);
        const half = Math.ceil(json.length / 2);
        event("content_block_delta", { index, delta: { type: "input_json_delta", partial_json: json.slice(0, half) } });
        event("content_block_delta", { index, delta: { type: "input_json_delta", partial_json: json.slice(half) } });
      }
      event("content_block_stop", { index });
    });
    event("message_delta", { delta: { stop_reason: answer.stop_reason }, usage: { output_tokens: answer.usage.output_tokens } });
    event("message_stop", {});
    return events;
  },
  openai: (answer) => {
    const choice = answer.choices[0];
    const chunk = (delta, finish_reason = null, usage) => `data: ${JSON.stringify({ id: "chatcmpl-scripted", choices: [{ index: 0, delta, finish_reason }], usage: usage ?? null })}\n\n`;
    const events = [chunk({ role: "assistant" })];
    if (choice.message.content) for (const piece of words(choice.message.content)) events.push(chunk({ content: piece }));
    (choice.message.tool_calls ?? []).forEach((call, index) => {
      const args = call.function.arguments;
      const half = Math.ceil(args.length / 2);
      events.push(chunk({ tool_calls: [{ index, id: call.id, type: "function", function: { name: call.function.name, arguments: args.slice(0, half) } }] }));
      events.push(chunk({ tool_calls: [{ index, function: { arguments: args.slice(half) } }] }));
    });
    events.push(chunk({}, choice.finish_reason));
    events.push(`data: ${JSON.stringify({ id: "chatcmpl-scripted", choices: [], usage: answer.usage })}\n\n`, "data: [DONE]\n\n");
    return events;
  },
  gemini: (answer) => {
    const candidate = answer.candidates[0];
    const chunk = (parts, last) => `data: ${JSON.stringify({ candidates: [{ content: { role: "model", parts }, ...(last && { finishReason: candidate.finishReason }) }], ...(last && { usageMetadata: answer.usageMetadata }) })}\n\n`;
    const events = [];
    const parts = candidate.content.parts;
    parts.forEach((part, i) => {
      const last = i === parts.length - 1;
      if (typeof part.text === "string") {
        const pieces = words(part.text);
        pieces.forEach((piece, j) => events.push(chunk([{ ...part, text: piece }], last && j === pieces.length - 1)));
      } else events.push(chunk([part], last));
    });
    return events;
  },
};

/** `text` in pieces, a word and its space each. */
function words(text) {
  return text.match(/\S+\s*|\s+/g) ?? [text];
}

/** A scripted provider on a port of its own, answering as the browser asks across origins:
 * each request it took; `slow`, to answer only after a while; and `trickle`, to send the first
 * event of a streamed answer, then wait that long before the rest. */
async function provider(wire) {
  const requests = [];
  /** `busy`: that many requests answered 429 with Retry-After: 1 before the answer; `hang`: that
   * many whose answer stops after its first word, the socket left open, before the answer
   * (narrative REL-3); `long`: that many answered 400, the prompt too long (lift-up PERF-3). */
  const state = { slow: 0, trickle: 0, busy: 0, hang: 0, long: 0 };
  const hung = [];
  const server = createServer(async (req, res) => {
    const cors = {
      "access-control-allow-origin": req.headers.origin ?? "*",
      "access-control-allow-headers": req.headers["access-control-request-headers"] ?? "*",
      "access-control-allow-methods": "GET, POST, OPTIONS",
    };
    if (req.method === "OPTIONS") return res.writeHead(204, cors).end();
    const path = new URL(req.url, "http://x").pathname;
    const reply = (status, body) => res.writeHead(status, { ...cors, "content-type": "application/json" }).end(JSON.stringify(body));
    if (req.method === "GET" && path === wire.models.path) {
      // A key the provider refuses, as Anthropic answers one.
      if (wire.key(req.headers) === "refused-key") return reply(401, { type: "error", error: { type: "authentication_error", message: "invalid x-api-key" } });
      return reply(200, wire.models.body);
    }
    if (req.method !== "POST" || path !== wire.turn) return reply(404, { error: { message: `no ${req.method} ${path}` } });
    let text = "";
    for await (const chunk of req) text += chunk;
    const body = JSON.parse(text);
    requests.push({ headers: req.headers, body });
    if (state.busy > 0) {
      state.busy--;
      return res.writeHead(429, { ...cors, "content-type": "application/json", "retry-after": "1" }).end(JSON.stringify({ error: { type: "rate_limit_error", message: "Rate limited" } }));
    }
    if (state.long > 0) {
      state.long--;
      return reply(400, { type: "error", error: { type: "invalid_request_error", message: "prompt is too long: 214012 tokens > 200000 maximum" } });
    }
    if (state.slow) await new Promise((ok) => setTimeout(ok, state.slow));
    const i = wire.answered(body);
    const answer = i < SCRIPT.length ? wire.call(SCRIPT[i], i) : wire.answer();
    // Streamed, as the page asks (narrative ATTR-3): each event written as it is, the first word
    // held apart by `trickle` so the page is seen showing it before the rest.
    if (!(body.stream || path.endsWith(":streamGenerateContent"))) return reply(200, answer);
    const id = Object.keys(WIRE).find((k) => WIRE[k] === wire);
    const events = SSE[id](answer);
    res.writeHead(200, { ...cors, "content-type": "text/event-stream", "cache-control": "no-cache" });
    // The first event with a word in it: Anthropic's block start carries an empty text.
    const first = events.findIndex((e) => /text_delta|"content":"[^"]|"text":"(?!Looking)[^"]/.test(e)) + 1;
    res.write(events.slice(0, Math.max(first, 1)).join(""));
    if (state.hang > 0) {
      state.hang--;
      return void hung.push(res);
    }
    if (state.trickle) await new Promise((ok) => setTimeout(ok, state.trickle));
    res.end(events.slice(Math.max(first, 1)).join(""));
  }).listen(0, "127.0.0.1");
  await new Promise((ok) => server.once("listening", ok));
  return {
    origin: `http://127.0.0.1:${server.address().port}`,
    requests,
    state,
    close: () => {
      for (const res of hung) res.end();
      server.close();
    },
  };
}

const site = await serve();
const browser = await launch();
try {
  /** The editor on the bundle, in a browser of its own: the page before it left the assistant's
   * edits not saved, and a page that opened the deck again beside it would be offered them first
   * (lift-up REL-4). */
  const open = async () => {
    const context = await browser.newContext({ viewport: { width: 1600, height: 1000 }, deviceScaleFactor: 1 });
    const page = await context.newPage();
    page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
    page.on("console", (m) => m.type() === "error" && !/Failed to load resource/.test(m.text()) && failures.push(`console: ${m.text()}`));
    // A turn gives up on silence after 3 s here, not two minutes (narrative REL-3).
    await page.goto(`${site.origin}/web/dist/editor.html?painter=cpu&source=open&bundle=/${bundle}/deck.json&assistant-timeout=3000`);
    await page.waitForFunction(() => window.scaena?.last(), null, { timeout: 120000 });
    await tab(page, "assistant");
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
      // The deck's facts come last, uncached, so an edit leaves the rest cached, and the
      // conversation is cached to its newest block (lift-up PERF-2).
      const facts = first.body.system[1];
      check(
        first.body.system.length === 2 && !facts.cache_control && facts.text.startsWith("\n\n--- the deck open ---\nThe deck: ") && !first.body.system[0].text.includes("The deck: "),
        `anthropic: the deck's facts are a block of their own, last and uncached (${JSON.stringify(facts?.text?.slice(0, 60))})`,
      );
      check(
        requests.every((r) => r.body.system[0].text === first.body.system[0].text),
        "anthropic: what is cached of the system prompt is the same on every round",
      );
      check(
        requests.every((r) => r.body.messages.at(-1).content.at(-1).cache_control?.type === "ephemeral" && r.body.messages.slice(0, -1).every((m) => m.content.every((c) => !c.cache_control))),
        "anthropic: each request caches the conversation to its newest block, and only there",
      );
    }
    const system = wire.system(first.body);
    check(system.includes("# author-deck") && system.includes("scaena://spec"), `${id}: the system prompt holds the author-deck skill and the resources`);
    const tools = wire.tools(first.body);
    const names = tools.map((t) => t.name);
    check(
      ["deck_read", "deck_patch", "deck_lint", "deck_inspect", "deck_diff", "deck_render", "spine_read", "spine_update", "deck_health", "data_attach", "data_edit", "theme_edit", "icon_find", "resource_read"].every((n) => names.includes(n)),
      `${id}: the tools: ${names.join(", ")}`,
    );
    check(
      system.includes("read its health with deck_health and leave each slide saying its beat's claim"),
      `${id}: the system prompt says to read the story before the deck is called done`,
    );
    check(
      tools.every((t) => !t.schema.properties?.bundle && !t.schema.properties?.out && !JSON.stringify(t.schema).includes("$ref")),
      `${id}: no tool takes a bundle or an output path, and every schema is whole`,
    );

    // Each result reaches the model fenced as data under its tool's name (lift-up SEC-3): read back
    // from its fence, its escaped closings as written.
    const fence = /^<data label="([a-z0-9-]+)">\n([\s\S]*)\n<\/data>$/;
    const sent = (n) => wire.results(requests[n].body)[0] ?? {};
    const result = (n) => {
      const r = sent(n);
      const m = r.json?.match(fence);
      return m ? { ...r, json: m[2].replace(/<\\\/(data)/gi, "</$1"), label: m[1] } : r;
    };
    const parsed = (n) => JSON.parse(result(n).json ?? "{}");
    check(
      [result(1).label, result(2).label, result(6).label, result(9).label].join() === "deck-patch,deck-lint,theme-edit,deck-health",
      `${id}: each tool's result reaches the model fenced under its name (${sent(1).json?.slice(0, 40)})`,
    );
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
    const icons = parsed(7);
    const rocket = icons.icons?.[0];
    check(
      icons.icons?.length === 2 && rocket?.name === "rocket" && JSON.stringify(rocket.icon?.view) === "[0,0,256,256]" && rocket.icon.path?.startsWith("M") && icons.found >= 2,
      `${id}: icon_find finds the rocket by its words, its path on its view (${result(7).json?.slice(0, 160)})`,
    );
    // Each call headed by what it came to: the patch that was made edited the deck; the one the
    // deck refused did not, and says so.
    check(!JSON.parse(result(8).json ?? "{}").applied, `${id}: the patch naming a layout the theme has not is not made (${result(8).json?.slice(0, 160)})`);
    const health = parsed(9);
    check(
      !result(9).error && Number.isInteger(health.score) && health.slides?.length === 1 && Array.isArray(health.findings),
      `${id}: deck_health reads the deck's story, by the generator's module: ${result(9).json?.slice(0, 160)}`,
    );
    const heads = await page.evaluate(() => [...document.querySelectorAll("#transcript li.call .doing")].map((d) => d.textContent));
    check(heads[0] === "Edited the deck" && heads[5] === "Edited the theme", `${id}: a call made is headed by what it did: ${heads[0]} · ${heads[5]}`);
    check(heads[7] === "Didn't edit the deck", `${id}: a call refused is headed as not done: ${heads[7]}`);
    const stories = await page.evaluate(() => [...document.querySelectorAll("#transcript li.call")].map((li) => [li.querySelector(".doing")?.textContent, li.querySelector(".outcome")?.textContent]));
    check(
      stories[8]?.[0] === "Read the deck's story" && /^Story \d+ of 100, \d+ findings?$/.test(stories[8]?.[1] ?? ""),
      `${id}: the story read is headed and summed up: ${stories[8]?.join(" · ")}`,
    );
    if (id === "gemini") {
      const model = requests[1].body.contents.find((c) => c.role === "model");
      check(model?.parts?.[0]?.thoughtSignature === "signature-0", "gemini: a call's thought signature goes back as it came");
    }

    // The Story tab's Ask puts its question to the assistant, which now has the tool to answer it
    // (narrative FUNC-2): the question in the box, the tab shown.
    if (id === "anthropic") {
      await tab(page, "story");
      await page.waitForFunction(() => document.querySelector("#story [data-ask]"), null, { timeout: 60000 });
      // The slide it is about is shown first (lift-up USE-5), then the question goes in the box.
      await page.evaluate(() => document.querySelector("#story [data-ask]").click());
      await page.waitForFunction(() => document.querySelector("#question").value.length > 20, null, { timeout: 30000 }).catch(() => {});
      const asked = await page.evaluate(() => ({ question: document.querySelector("#question").value, shown: document.querySelector("#tab-assistant").getAttribute("aria-selected") }));
      check(asked.shown === "true" && asked.question.length > 20, `the Story tab's Ask puts its question to the assistant: ${asked.question.slice(0, 80)}`);
    }

    await settled(page);
    const after = await page.evaluate(() => window.scaena.source());
    check(after !== before && after.includes(HEADLINE), `${id}: the editor's source took each edit`);
    const last = await page.evaluate(() => window.scaena.last());
    check(last.valid && !last.findings.some((f) => f.severity === "error"), `${id}: and lints clean: ${last.findings.map((f) => f.code).join(", ") || "nothing found"}`);
    const transcript = await page.evaluate(() => window.scaena.assistant.transcript());
    check(
      transcript.filter((t) => t.startsWith("call")).length === SCRIPT.length && transcript.at(-1) === `said: ${PLAIN}`,
      `${id}: the conversation shows each call and the answer (${transcript.length} entries): ${transcript.at(-1)}`,
    );
    // The answer as it reads (narrative ATTR-4): its list and emphasis as elements, never markup
    // from the model's text, and the state named in a code span a link that shows it.
    const rendered = await page.evaluate(() => {
      const li = [...document.querySelectorAll("#transcript li.said")].at(-1);
      return { items: li.querySelectorAll("ul li").length, strong: li.querySelector("strong")?.textContent, link: li.querySelector("a.state")?.dataset.state, raw: li.textContent.includes("**") };
    });
    check(rendered.items === 2 && rendered.strong === "start" && rendered.link === "start" && !rendered.raw, `${id}: the answer reads as a list, bold, and a link to the slide: ${JSON.stringify(rendered)}`);
    const shown = await page.$eval("#transcript img", (img) => img.naturalWidth).catch(() => 0);
    check(shown === 960, `${id}: and the frame the model saw`);
    // Under each call that changed what a state draws, that state before and after (PLAN 2.93):
    // the patch drew the headline on start, and lint's fix set it to fit, the first's after the
    // second's before; a lint and a render draw nothing anew.
    const changes = await page.evaluate(() =>
      [...document.querySelectorAll("#transcript li.call")].map((li) =>
        [...li.querySelectorAll(".changed .change")].map((b) => {
          const sum = (c) => (c ? [...c.getContext("2d").getImageData(0, 0, c.width, c.height).data].reduce((a, v) => (a * 31 + v) % 1000000007, 0) : null);
          const [before, after] = [...b.querySelectorAll("canvas")];
          return { state: b.dataset.state, before: sum(before), after: sum(after) };
        }),
      ),
    );
    const of = (i) => changes[i] ?? [];
    const [patched2, fixed2] = [of(0)[0], of(2)[0]];
    check(of(0).length === 1 && patched2.state === "start" && patched2.before !== null && patched2.before !== patched2.after, `${id}: the patch shows start before and after: ${JSON.stringify(of(0))}`);
    check(of(2).length === 1 && fixed2.state === "start" && fixed2.before === patched2.after && fixed2.after !== fixed2.before, `${id}: lint's fix shows start as the patch left it, then fixed: ${JSON.stringify(of(2))}`);
    check([1, 3, 4].every((i) => of(i).length === 0), `${id}: a lint and a render show nothing drawn anew`);

    // The theme's edit shows in the Theme tab, and is one step of the source's undo: ⌘Z writes the
    // theme back (PLAN 2.61).
    const accent = () => page.evaluate(() => window.scaena.theme.theme()?.tokens?.color?.accent);
    await tab(page, "theming");
    await page.waitForFunction(() => window.scaena.theme.theme()?.tokens?.color?.accent === "#2E86E4", null, { timeout: 30000 }).catch(() => {});
    check((await accent()) === "#2E86E4", `${id}: the Theme tab shows the assistant's edit: ${await accent()}`);
    await page.locator("#code .cm-content").focus();
    await page.keyboard.press("Control+z");
    await page.waitForFunction(() => window.scaena.theme.theme()?.tokens?.color?.accent === "#FF6A3D", null, { timeout: 30000 }).catch(() => {});
    check((await accent()) === "#FF6A3D" && (await page.evaluate(() => window.scaena.source())) === after, `${id}: ⌘Z writes the theme back, the source as it was`);
    await tab(page, "assistant");
    const usage = await page.evaluate(() => window.scaena.assistant.usage());
    check(usage.input === 1000 * SCRIPT.length + 1200, `${id}: tokens counted: ${usage.input} in, ${usage.output} out`);

    if (id === "anthropic") {
      // A question asked again stops when the user says so, and the source takes typing again.
      scripted.state.slow = 4000;
      // Stop shows as the question is asked, while the strip finishes drawing and before the
      // worker has it: a press then stops it too.
      const early = await page.evaluate(() => {
        const asked = window.scaena.assistant.ask("Make it shorter.");
        document.querySelector("#stop").click();
        return asked;
      });
      check(early.kind === "done" && early.stop === "stopped", `Stop pressed before the question is sent stops it (${JSON.stringify(early)})`);
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
    }

    // The answer's words show as they are written (narrative ATTR-3): the scripted provider
    // sends the answer's first word, then holds the rest, and the transcript shows that word
    // while the assistant still works; whole, the answer stands in its place.
    {
      scripted.state.trickle = 2500;
      // The entries said so far: the words show in a new one, not in the last answer's.
      const before = await page.evaluate(() => document.querySelectorAll("#transcript li.said").length);
      await page.evaluate(() => {
        window.asked = window.scaena.assistant.ask("Say that again.");
      });
      const partial = await page
        .waitForFunction(
          (before) => {
            const saids = document.querySelectorAll("#transcript li.said");
            const said = saids.length > before ? (saids[saids.length - 1].textContent ?? "") : "";
            return said.length > 0 && !document.querySelector("#stop").hidden && said;
          },
          before,
          { timeout: 10000 },
        )
        .then((h) => h.jsonValue())
        .catch(() => "");
      check(partial && partial.length < ANSWER.length && ANSWER.startsWith(partial), `${id}: the first words show before the answer is whole: "${partial}"`);
      const again = await page.evaluate(() => window.asked);
      check(again.kind === "done" && again.stop === "end", `${id}: the streamed answer ends (${JSON.stringify(again)})`);
      const saids = await page.evaluate(() => [...document.querySelectorAll("#transcript li.said")].map((li) => li.textContent));
      check(saids.at(-1) === PLAIN && saids.filter((t) => t === PLAIN).length === 2, `${id}: the answer stands whole in the words' place, once: ${JSON.stringify(saids.slice(-2))}`);
      scripted.state.trickle = 0;
    }

    // A busy provider is asked again (narrative REL-3): two 429s with Retry-After: 1, the wait
    // said where Working… was, then the answer; and one that goes silent is given up on at the
    // deadline and asked again, the question still answered.
    {
      const before = scripted.requests.length;
      scripted.state.busy = 2;
      await page.evaluate(() => {
        window.asked = window.scaena.assistant.ask("Once more.");
      });
      const waited = await page
        .waitForFunction(() => /Trying again in \d+ s \(1 of 3\)/.test(document.querySelector("#thinking").textContent) && document.querySelector("#thinking").textContent, null, { timeout: 10000 })
        .then((h) => h.jsonValue())
        .catch(() => "");
      check(/^(Anthropic|OpenAI|Gemini) is busy\. Trying again in \d+ s \(1 of \d+\)…$/.test(waited), `${id}: the wait is said in a person's words: "${waited}"`);
      const again = await page.evaluate(() => window.asked);
      check(again.kind === "done" && again.stop === "end" && scripted.requests.length === before + 3, `${id}: asked again after each 429, then answered (${JSON.stringify(again)}, ${scripted.requests.length - before} requests)`);
      const quiet = scripted.requests.length;
      scripted.state.hang = 1;
      const silent = await page.evaluate(() => window.scaena.assistant.ask("And again."));
      check(silent.kind === "done" && silent.stop === "end" && scripted.requests.length === quiet + 2, `${id}: an answer that stops in the middle is asked again at the deadline, and answers (${JSON.stringify(silent)}, ${scripted.requests.length - quiet} requests)`);
      // A provider that thinks for longer than the deadline before its answer begins is waited
      // for, not asked again: the deadline counts once the answer has begun.
      const thinking = scripted.requests.length;
      scripted.state.slow = 4500;
      const thought = await page.evaluate(() => window.scaena.assistant.ask("Think first."));
      scripted.state.slow = 0;
      check(thought.kind === "done" && thought.stop === "end" && scripted.requests.length === thinking + 1, `${id}: a provider that thinks before it answers is waited for (${JSON.stringify(thought)}, ${scripted.requests.length - thinking} requests)`);
      // Stop during the wait stops at once.
      scripted.state.busy = 3;
      await page.evaluate(() => {
        window.asked = window.scaena.assistant.ask("Yet again.");
      });
      await page.waitForFunction(() => /Trying again/.test(document.querySelector("#thinking").textContent), null, { timeout: 10000 }).catch(() => {});
      const stoppedAt = Date.now();
      await page.click("#stop");
      const stopped = await page.evaluate(() => window.asked);
      check(stopped.kind === "done" && stopped.stop === "stopped" && Date.now() - stoppedAt < 1500, `${id}: Stop during the wait stops at once (${JSON.stringify(stopped)})`);
      scripted.state.busy = 0;
    }

    // A conversation too long for the model is sent again once with half as much of tools'
    // results, never more than it sent, and answered (lift-up PERF-3); a second refusal is said.
    {
      const before = scripted.requests.length;
      scripted.state.long = 1;
      const shorter = await page.evaluate(() => window.scaena.assistant.ask("Shorter now."));
      const sent = (r) => JSON.stringify(r.body.messages ?? r.body.contents).length;
      const [long, again] = scripted.requests.slice(before);
      check(
        shorter.kind === "done" && shorter.stop === "end" && scripted.requests.length === before + 2 && sent(again) <= sent(long),
        `${id}: a conversation too long is asked again once, sending no more than it did, and answered (${JSON.stringify(shorter)}, ${scripted.requests.length - before} requests)`,
      );
      const asked = scripted.requests.length;
      scripted.state.long = 2;
      const refused = await page.evaluate(() => window.scaena.assistant.ask("Still shorter."));
      scripted.state.long = 0;
      check(
        refused.kind === "failed" && /more than .*'s model takes at once/.test(refused.message) && scripted.requests.length === asked + 2,
        `${id}: and too long again, it is said, once asked again (${JSON.stringify(refused)}, ${scripted.requests.length - asked} requests)`,
      );
    }

    if (id === "anthropic") {
      // The key's field: masked, shown when asked; its form said as it is typed; checked with the
      // provider, which lists its models or refuses it.
      const keyState = () => page.$eval("#keystatus", (p) => ({ state: p.dataset.state, text: p.textContent }));
      const keyed = (state) => page.waitForFunction((s) => document.querySelector("#keystatus").dataset.state === s, state, { timeout: 15000 }).catch(() => {});
      await keyed("works");
      let said = await keyState();
      check(said.state === "works" && said.text.includes("1 model"), `the key is said to work, with its models: ${said.text}`);
      check((await page.$eval("#key", (k) => k.type)) === "password", "the key is masked");
      await page.click("#reveal-key");
      check(
        (await page.$eval("#key", (k) => k.type)) === "text" && (await page.$eval("#reveal-key", (b) => b.getAttribute("aria-pressed"))) === "true",
        "Show the key shows it",
      );
      await page.click("#reveal-key");
      check((await page.$eval("#key", (k) => k.type)) === "password", "and masks it again");
      await page.fill("#key", "AIzaSyB-a-key-of-geminis-form-0123");
      const hint = await page.$eval("#keyhint", (p) => (p.hidden ? "" : p.textContent));
      check(hint === "This looks like a Gemini key.", `a key of another form is said so as it is typed: ${hint}`);
      await page.fill("#key", "refused-key");
      await keyed("refused");
      said = await keyState();
      // In a person's words (PLAN 3.18), as the Mac's Keys say it: not the provider's own, nor its status.
      check(
        said.state === "refused" && said.text === "Anthropic didn't accept your key. Check it, or replace it.",
        `a key the provider refuses is said so, in a person's words: ${said.text}`,
      );
      check((await page.$eval("#model", (s) => s.options.length)) === 0, "and offers no models");
      await page.fill("#key", "anthropic-test-key");
      await keyed("works");
      // The test's key is not of Anthropic's form: it is warned of, and works all the same, since
      // the form warns and refuses nothing; the provider's answer decides.
      const warned = await page.$eval("#keyhint", (p) => (p.hidden ? "" : p.textContent));
      check((await keyState()).state === "works" && warned === "Anthropic's keys start with sk-ant-.", `a key of another form that the provider takes works, warned of: ${warned}`);

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
      // The editor offers the copy it kept of what the assistant changed and ⌘Z took back, the
      // theme written twice (lift-up REL-4), a modal over the page: discarded, since it is the
      // key that is checked here.
      await page
        .waitForSelector("#recovering[open]", { timeout: 5000 })
        .then(() => page.click("#recovering [value=discard]"))
        .catch(() => {});
      await tab(page, "assistant");
      await page.waitForFunction(() => document.querySelector("#key").value, null, { timeout: 10000 });
      check((await page.$eval("#key", (k) => k.value)) === "anthropic-test-key", "and comes back after a reload");
      // The vault not reached (IndexedDB refused, as in a private window): the key is not read,
      // and kept, and read again once the vault is reached (lift-up 10-24b REL-5).
      await page.addInitScript(() => {
        if (sessionStorage.getItem("scaena.test.vault") !== "unreached") return;
        indexedDB.open = () => {
          const asked = {};
          setTimeout(() => asked.onerror?.(new Event("error")));
          return asked;
        };
      });
      const reopened = async () => {
        await page.reload();
        await page.waitForFunction(() => window.scaena?.last(), null, { timeout: 120000 });
        await page
          .waitForSelector("#recovering[open]", { timeout: 5000 })
          .then(() => page.click("#recovering [value=discard]"))
          .catch(() => {});
        await tab(page, "assistant");
      };
      await page.evaluate(() => sessionStorage.setItem("scaena.test.vault", "unreached"));
      await reopened();
      await page.waitForTimeout(1500);
      const unreached = await stored();
      check(
        (await page.$eval("#key", (k) => k.value)) === "" && unreached.local === keys.local,
        "with the vault not reached, the key is not read, and kept as it was",
      );
      await page.evaluate(() => sessionStorage.removeItem("scaena.test.vault"));
      await reopened();
      await page.waitForFunction(() => document.querySelector("#key").value, null, { timeout: 10000 });
      check((await page.$eval("#key", (k) => k.value)) === "anthropic-test-key", "and read again once it is");
      await page.click("#forget-key");
      keys = await stored();
      check(keys.session === null && keys.local === null && (await page.$eval("#key", (k) => k.value)) === "", "Forget key forgets it everywhere");
      said = await keyState();
      check(said.state === "none" && said.text.includes("No key yet"), `and says there is no key: ${said.text}`);
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

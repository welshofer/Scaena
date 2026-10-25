// The assistant's providers, as the browser's adapters ask them (PLAN 2.6, 3.6, ADR-0022): one
// conversation (tests/assistant/conversation.json), each provider's request for it, and its
// answer read back; a question asked once with nothing to call, as a writing tool asks it (PLAN
// 2.101), and one asked of a picture, as Describe asks it (PLAN 2.119); and what the model is told, the system prompt for a deck and the line that says what
// the editor shows. With --write, the record is written to
// tests/assistant/exchanges.json; without, the browser's code is held to the record, which
// scaena-chat's tests hold the Rust code to as well: the two cannot drift apart unseen.
//
//   node --experimental-strip-types web/exchanges.mjs [--write]   (from the repository's root)
//
// Exits 1 on any failure. Node reads the adapters' TypeScript as it stands, its types stripped.
import { readFile, writeFile } from "node:fs/promises";
import { isDeepStrictEqual } from "node:util";
import { RULE, fenced, seen, system } from "./src/assistant/prompt.ts";
import { CUT, owing } from "./src/assistant/cut.ts";
import { providers, refusedBase } from "./src/assistant/providers.ts";

const write = process.argv.includes("--write");
const conversation = JSON.parse(await readFile("tests/assistant/conversation.json", "utf8"));
const recordPath = "tests/assistant/exchanges.json";

/** `text` as a body that arrives in pieces of 97 bytes, each cut wherever it falls. */
function pieces(text) {
  const bytes = new TextEncoder().encode(text);
  let at = 0;
  return new ReadableStream({
    pull(controller) {
      if (at >= bytes.length) return controller.close();
      controller.enqueue(bytes.slice(at, at + 97));
      at += 97;
    },
  });
}

/** Each request `run` makes, and the answer `reply` gives it, by a `fetch` that calls nothing. */
async function asking(reply, run) {
  const asked = [];
  const real = globalThis.fetch;
  globalThis.fetch = async (url, init = {}) => {
    asked.push({
      method: init.method ?? "GET",
      url: String(url),
      headers: Object.fromEntries(Object.entries(init.headers ?? {}).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))),
      body: init.body === undefined ? null : JSON.parse(init.body),
    });
    const { status, body } = reply;
    return new Response(typeof body === "string" || body instanceof ReadableStream ? body : JSON.stringify(body), { status, statusText: "" });
  };
  try {
    return { asked, result: await run().then((value) => ({ value }), (e) => ({ error: e.message })) };
  } finally {
    globalThis.fetch = real;
  }
}

const record = {};
for (const [id, provider] of Object.entries(providers)) {
  const ask = {
    model: conversation.model,
    key: conversation.key,
    system: conversation.system,
    tools: conversation.tools,
    messages: conversation.messages,
  };
  const turn = await asking({ status: 200, body: conversation.answers[id] }, () => provider.turn(ask));
  // The same answer as the provider streams it (narrative ATTR-3), the body handed over in
  // pieces that cut its events mid-way: read as the whole one is, its words out as they come.
  const said = [];
  const streamed = await asking({ status: 200, body: pieces(conversation.streamed[id]) }, () => provider.turn({ ...ask, onText: (text) => said.push(text) }));
  const models = await asking({ status: 200, body: conversation.models[id] }, () => provider.models(conversation.key));
  const failure = await asking(conversation.failures[id], () => provider.turn(ask));
  // A question asked once, with nothing to call: a writing tool's (PLAN 2.101).
  const once = { model: conversation.model, key: conversation.key, system: conversation.once.system, tools: [], messages: [{ role: "user", text: conversation.once.prompt }] };
  const asked = await asking({ status: 200, body: conversation.answers[id] }, () => provider.turn(once));
  // One asked of a picture, shown before its words: Describe's (PLAN 2.119).
  const { picture } = conversation;
  const pictured = { ...once, system: picture.system, messages: [{ role: "user", text: picture.prompt, png: picture.png }] };
  const shown = await asking({ status: 200, body: conversation.answers[id] }, () => provider.turn(pictured));
  record[id] = {
    turn: { request: turn.asked[0], read: turn.result.value, streamed: { read: streamed.result.value, said } },
    models: { request: models.asked[0], read: models.result.value },
    failure: { message: failure.result.error },
    once: { request: asked.asked[0] },
    picture: { request: shown.asked[0] },
  };
}

const told = conversation.prompt;
record.prompt = {
  system: told.decks.map((deck, i) => system(deck, told.resources, told.bundleSkills[i], told.authorDeck)),
  seen: told.seeings.map((seeing) => seen(seeing ?? undefined)),
  // The fence, as scaena-fence writes it (lift-up SEC-3): the rule, and each text fenced.
  fence: { rule: RULE, fenced: (told.fenced ?? []).map(([label, text]) => fenced(label, text)) },
};

const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};
if (write) {
  await writeFile(recordPath, `${JSON.stringify(record, null, 2)}\n`);
  console.log(`wrote ${recordPath}`);
} else {
  const kept = JSON.parse(await readFile(recordPath, "utf8"));
  for (const id of Object.keys(providers)) {
    for (const part of ["turn", "models", "failure", "once", "picture"]) {
      check(isDeepStrictEqual(record[id][part], kept[id][part]), `${id}: the ${part} the adapter makes is the one recorded`);
    }
  }
  for (const part of ["system", "seen", "fence"]) {
    const what = { system: "system prompt", seen: "line saying what the editor shows", fence: "fence" }[part];
    check(isDeepStrictEqual(record.prompt[part], kept.prompt?.[part]), `the ${what} is the one recorded`);
  }
}
// The API addresses a key may go to, held to the cases scaena-narrato's and scaena-chat's
// `checked_base` are (lift-up 20261025 SEC-2).
const bases = JSON.parse(await readFile("tests/assistant/bases.json", "utf8"));
for (const good of bases.good) check(refusedBase(good) === undefined, `a key may go to ${JSON.stringify(good)}`);
for (const bad of bases.bad) check(refusedBase(bad) !== undefined, `a key never goes to ${JSON.stringify(bad)}`);
// A call the answer's token limit cut off is answered, not run (lift-up REL-1), as scaena-chat's
// `Chat::answer` answers it: the browser's rule, which converse.ts takes.
{
  const lint = { id: "toolu_1", name: "deck_lint", args: { state: "cover" } };
  const { run, owed } = owing([lint, { id: "toolu_2", name: "deck_patch", args: {} }], "length");
  check(run.length === 1 && run[0].id === "toolu_1", "a cut call: only the whole call before it runs");
  check(owed.length === 1 && owed[0].id === "toolu_2" && owed[0].error, "and the cut one is owed an answer, as an error");
  check(owed[0]?.json === JSON.stringify({ message: CUT }), "told to make the edit in smaller patches");
  const alone = owing([{ id: "call_1", name: "deck_patch", args: '{"ops": [{"op": "repl' }], "length");
  check(alone.run.length === 0 && alone.owed.length === 1, "a cut call alone, its arguments no JSON, runs nothing and is owed an answer");
  const render = { id: "toolu_3", name: "deck_render", args: {} };
  check(owing([render], "tools").run.length === 1, "a call with no arguments in a whole answer runs");
  check(owing([render, lint], "length").owed.length === 0, "a last call with its arguments whole runs");
}
if (failures.length) {
  console.log(`\n${failures.length} failed: the browser's assistant no longer says what is recorded. If the change is meant, write it again (--write), and hold scaena-chat to it.`);
  process.exit(1);
}

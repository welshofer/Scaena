// The assistant's providers, as the browser's adapters ask them (PLAN 2.6, 3.6, ADR-0022): one
// conversation (tests/assistant/conversation.json), each provider's request for it, and its
// answer read back; and what the model is told, the system prompt for a deck and the line that
// says what the editor shows. With --write, the record is written to
// tests/assistant/exchanges.json; without, the browser's code is held to the record, which
// scaena-chat's tests hold the Rust code to as well: the two cannot drift apart unseen.
//
//   node --experimental-strip-types web/exchanges.mjs [--write]   (from the repository's root)
//
// Exits 1 on any failure. Node reads the adapters' TypeScript as it stands, its types stripped.
import { readFile, writeFile } from "node:fs/promises";
import { isDeepStrictEqual } from "node:util";
import { seen, system } from "./src/assistant/prompt.ts";
import { providers } from "./src/assistant/providers.ts";

const write = process.argv.includes("--write");
const conversation = JSON.parse(await readFile("tests/assistant/conversation.json", "utf8"));
const recordPath = "tests/assistant/exchanges.json";

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
    return new Response(typeof body === "string" ? body : JSON.stringify(body), { status, statusText: "" });
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
  const models = await asking({ status: 200, body: conversation.models[id] }, () => provider.models(conversation.key));
  const failure = await asking(conversation.failures[id], () => provider.turn(ask));
  record[id] = {
    turn: { request: turn.asked[0], read: turn.result.value },
    models: { request: models.asked[0], read: models.result.value },
    failure: { message: failure.result.error },
  };
}

const told = conversation.prompt;
record.prompt = {
  system: told.decks.map((deck, i) => system(deck, told.resources, told.bundleSkills[i], told.authorDeck)),
  seen: told.seeings.map((seeing) => seen(seeing ?? undefined)),
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
    for (const part of ["turn", "models", "failure"]) {
      check(isDeepStrictEqual(record[id][part], kept[id][part]), `${id}: the ${part} the adapter makes is the one recorded`);
    }
  }
  for (const part of ["system", "seen"]) {
    check(isDeepStrictEqual(record.prompt[part], kept.prompt?.[part]), `the ${part === "system" ? "system prompt" : "line saying what the editor shows"} is the one recorded`);
  }
}
if (failures.length) {
  console.log(`\n${failures.length} failed: the browser's assistant no longer says what is recorded. If the change is meant, write it again (--write), and hold scaena-chat to it.`);
  process.exit(1);
}

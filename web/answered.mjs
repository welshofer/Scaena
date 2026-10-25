// Every call the model makes is answered, an error included (the 20261025b plan's STAB-3): a call
// that throws, as an edit the editor could not take does, goes back as a failed result, so the
// conversation holds no call without its result and the next turn is asked, rather than every turn
// after it refused by the provider. Node reads web/src/assistant/converse.ts as it stands, its
// types stripped, its imports written without their `.ts` found by the hook below.
//
//   node --experimental-strip-types web/answered.mjs   (from the repository's root)
//
// Exits 1 on any failure.
import { register } from "node:module";

register(
  `data:text/javascript,${encodeURIComponent(`
    export async function resolve(specifier, context, next) {
      try {
        return await next(specifier, context);
      } catch (e) {
        if (!specifier.startsWith(".") || specifier.endsWith(".ts")) throw e;
        return next(specifier + ".ts", context);
      }
    }`)}`,
);
const { converse } = await import("./src/assistant/converse.ts");

const failures = [];
const check = (ok, what) => (ok ? console.log(`ok   ${what}`) : (failures.push(what), console.log(`FAIL ${what}`)));

/** A provider that answers `turns` in order, and the messages it was sent each time. */
function scripted(turns) {
  const sent = [];
  return {
    sent,
    provider: {
      name: "Scripted",
      base: "",
      models: async () => ["scripted"],
      turn: async (ask) => {
        sent.push(structuredClone(ask.messages));
        return turns.shift() ?? { text: "Done.", calls: [], stop: "end" };
      },
    },
  };
}

/** One question asked of `provider`, each call run by `call`: the conversation and what was heard. */
async function asked(provider, call) {
  const conversation = [{ role: "user", text: "Make the title louder, then check it." }];
  const heard = [];
  await converse({
    provider,
    model: "scripted",
    key: "k",
    system: "",
    tools: [],
    conversation,
    call,
    emit: (event) => heard.push(event),
    signal: new AbortController().signal,
    retries: 0,
  });
  return { conversation, heard };
}

// Two calls in one turn: the first throws, as the editor's taking of an edit can; the second runs.
const { sent, provider } = scripted([
  {
    text: "Making it louder.",
    calls: [
      { id: "t1", name: "deck_patch", args: { ops: [] } },
      { id: "t2", name: "deck_lint", args: {} },
    ],
    stop: "tools",
  },
  { text: "Done.", calls: [], stop: "end" },
]);
const { conversation, heard } = await asked(provider, async (call) => {
  if (call.name === "deck_patch") throw new Error("the worker could not lay the deck out");
  return { result: { id: call.id, name: call.name, json: JSON.stringify({ findings: [] }), error: false }, summary: "Nothing found", made: true };
});

const tool = conversation.find((m) => m.role === "tool");
const ids = tool?.results.map((r) => r.id).join() ?? "";
check(ids === "t1,t2", `each call has its result, the one that threw too: ${ids}`);
const failed = tool?.results.find((r) => r.id === "t1");
check(failed?.error === true && failed.json.includes("the worker could not lay the deck out"), `the call that threw is answered as failed, saying why: ${failed?.json}`);
check(tool?.results.find((r) => r.id === "t2")?.error === false, "the call after it still runs");
check(sent.length === 2 && sent[1].some((m) => m.role === "tool"), `the next turn is asked, with the results: ${sent.length} turns`);
const result = heard.find((e) => e.kind === "result" && e.id === "t1");
check(result?.error === true && result.made === false, `the panel hears it as not done: ${JSON.stringify(result)}`);
const done = heard.at(-1);
check(done?.kind === "done" && done.stop === "end", `and the answer ends as answers do: ${JSON.stringify(done)}`);

// Asked again on the same conversation, every call in it has its result: a provider that refuses a
// call left unanswered finds none.
const calls = conversation.flatMap((m) => (m.role === "assistant" ? m.calls.map((c) => c.id) : []));
const answered = new Set(conversation.flatMap((m) => (m.role === "tool" ? m.results.map((r) => r.id) : [])));
check(calls.every((id) => answered.has(id)), `no call in the conversation is left without its result: ${calls.join()}`);

console.log(failures.length ? `${failures.length} failure(s)` : "every call the model makes is answered, an error included");
process.exit(failures.length ? 1 : 0);

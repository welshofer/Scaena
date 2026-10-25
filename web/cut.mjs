// A call cut off by the answer's token limit is not run (lift-up REL-1), in the browser as in
// scaena-chat's `Chat::answer`: it goes back with the others' results, told to make the edit in
// smaller patches, and a whole call before it runs as before. Node reads
// web/src/assistant/cut.ts as it stands, its types stripped; converse.ts takes what it says.
//
//   node --experimental-strip-types web/cut.mjs   (from the repository's root)
//
// Exits 1 on any failure.
import { CUT, owing } from "./src/assistant/cut.ts";

const failures = [];
const check = (ok, what) => (ok ? console.log(`ok   ${what}`) : (failures.push(what), console.log(`FAIL ${what}`)));

const lint = { id: "toolu_1", name: "deck_lint", args: { state: "cover" } };

// A whole call, then one the limit cut off, its input `{}` as a stream leaves it.
{
  const { run, owed } = owing([lint, { id: "toolu_2", name: "deck_patch", args: {} }], "length");
  check(run.length === 1 && run[0].id === "toolu_1", `only the whole call runs: ${JSON.stringify(run)}`);
  check(owed.length === 1 && owed[0].id === "toolu_2" && owed[0].error, "the cut call is owed an answer, as an error");
  check(owed[0]?.json === JSON.stringify({ message: CUT }), `told to make the edit in smaller patches: ${owed[0]?.json}`);
}
// A cut call alone, its arguments text that is no JSON, as OpenAI's are.
{
  const { run, owed } = owing([{ id: "call_1", name: "deck_patch", args: '{"ops": [{"op": "repl' }], "length");
  check(run.length === 0 && owed.length === 1 && owed[0].id === "call_1", "a cut call alone runs nothing and is owed an answer");
}
// An answer that did not run out of room runs every call, `{}` or not; one that did, with whole
// arguments last, runs them too.
{
  const render = { id: "toolu_3", name: "deck_render", args: {} };
  check(owing([render], "tools").run.length === 1, "a call with no arguments in a whole answer runs");
  check(owing([render, lint], "length").owed.length === 0, "a last call with its arguments whole runs");
  check(owing([], "length").owed.length === 0, "no call, nothing owed");
}

if (failures.length) {
  console.log(`\n${failures.length} failed`);
  process.exit(1);
}

// A call cut off by the answer's token limit is not run (lift-up REL-1): its arguments are not
// what the model meant. It goes back with the others' results, told to make the edit in smaller
// patches, as scaena-chat's `Chat::answer` tells it. Pure, with no import, so node checks it as
// it stands (`web/cut.mjs`).

/** What a call cut off by the answer's token limit is told in place of its result, as
 * scaena-chat's `CUT` says it. */
export const CUT = "Your answer ran out of room in this call; make the edit in smaller patches.";

interface Called {
  id: string;
  name: string;
  args: unknown;
}

/** An answer's `calls`, which stopped as `stop` says: those to run, and the results owed in place
 * of the last, where the answer ran out of room and its arguments came to nothing or did not read
 * as JSON, as scaena-chat's `cut` judges it. */
export function owing<C extends Called>(
  calls: C[],
  stop: string,
): { run: C[]; owed: { id: string; name: string; json: string; error: boolean }[] } {
  const last = calls.at(-1);
  const args = last?.args;
  const nothing =
    typeof args !== "object" || args === null || Array.isArray(args) || Object.keys(args).length === 0;
  if (stop !== "length" || !last || !nothing) return { run: calls, owed: [] };
  return {
    run: calls.slice(0, -1),
    owed: [{ id: last.id, name: last.name, json: JSON.stringify({ message: CUT }), error: true }],
  };
}
